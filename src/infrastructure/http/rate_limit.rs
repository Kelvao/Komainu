use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct RateLimiter {
    records: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    max_requests: u32,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_duration: Duration) -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_duration,
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let mut map = self.records.lock().unwrap();
        let now = Instant::now();
        if map.len() > 1000 {
            map.retain(|_, (_, start)| now.duration_since(*start) < self.window_duration);
        }
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window_duration {
            *entry = (1, now);
            true
        } else if entry.0 < self.max_requests {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}

pub async fn rate_limit_middleware(
    limiter: RateLimiter,
    req: Request,
    next: Next,
) -> Response {
    let client_ip = extract_client_ip(&req);
    if !limiter.check(&client_ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            "Too many requests. Please try again later.",
        )
            .into_response();
    }
    next.run(req).await
}

fn is_trusted_peer(addr: std::net::IpAddr) -> bool {
    if addr.is_loopback() {
        return true;
    }

    let trusted_env = match std::env::var("TRUSTED_PROXIES").or_else(|_| std::env::var("TRUST_PRIVATE_PROXIES")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    for item in trusted_env.split(',') {
        let trimmed = item.trim();
        if trimmed.eq_ignore_ascii_case("private") || trimmed == "1" || trimmed.eq_ignore_ascii_case("true") {
            match addr {
                std::net::IpAddr::V4(ipv4) if ipv4.is_private() => return true,
                std::net::IpAddr::V6(ipv6) if (ipv6.octets()[0] & 0xfe) == 0xfc => return true,
                _ => {}
            }
        } else if let Ok(trusted_ip) = trimmed.parse::<std::net::IpAddr>()
            && addr == trusted_ip {
                return true;
        }
    }

    false
}

pub(crate) fn extract_client_ip(req: &Request) -> String {
    let peer_addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());

    let is_trusted_proxy = match peer_addr {
        Some(addr) => is_trusted_peer(addr),
        None => true, // Fallback for synthetic requests in tests
    };

    if is_trusted_proxy {
        if let Some(forwarded) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok())
            && let Some(first) = forwarded.split(',').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        if let Some(real_ip) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let trimmed = real_ip.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    peer_addr.map(|a| a.to_string()).unwrap_or_else(|| "127.0.0.1".to_string())
}
