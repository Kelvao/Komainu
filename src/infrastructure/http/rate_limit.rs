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

pub(crate) fn extract_client_ip(req: &Request) -> String {
    let peer_ip = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string());

    let is_trusted_proxy = match peer_ip.as_deref() {
        Some(ip) => ip == "127.0.0.1" || ip == "::1",
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

    peer_ip.unwrap_or_else(|| "127.0.0.1".to_string())
}
