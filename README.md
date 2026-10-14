# Komainu (狛犬)

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.85%2B-orange?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Binary_Size-1.1_MB-success?style=for-the-badge" alt="Binary Size" />
  <img src="https://img.shields.io/badge/Auth-WebAuthn_%2F_Passkeys-blue?style=for-the-badge&logo=fido" alt="Passkeys" />
  <img src="https://img.shields.io/badge/Frontend-HTMX_%2B_Tailwind-informational?style=for-the-badge" alt="HTMX" />
  <img src="https://img.shields.io/badge/License-MIT-lightgrey?style=for-the-badge" alt="License" />
</p>

> **Komainu (狛犬)** are stone lion-dog guardian statues traditionally positioned at the entrance of Japanese Shinto shrines to ward off evil spirits.
> In that same spirit, **Komainu** serves as a lightweight, secure, and battle-tested single-binary guardian for your self-hosted **[Vaultwarden](https://github.com/dani-garcia/vaultwarden)** Docker instances.

---

## Overview

Self-hosting Vaultwarden gives you complete control over your passwords and secrets, but managing the underlying container typically means either relying on raw SSH commands or deploying heavyweight container management platforms (like Portainer), which introduce unwanted complexity, memory overhead, and security exposure.

**Komainu** solves this with a hyper-focused, single-binary management suite:
- **Zero runtime dependencies**: Compiles into a single standalone executable (~1.1 MB with UPX compression).
- **Passwordless Security**: Hardware-backed Passkey (WebAuthn/FIDO2) authentication only.
- **Zero Public Registration**: No web registration forms. The admin Passkey is registered strictly via CLI over SSH.
- **Live Monitoring & Control**: Real-time host and container resource inspection paired with safe lifecycle controls (Start, Stop, Restart, Pull Update, Rebuild).
- **Embedded UI**: Pure HTMX and Tailwind CSS rendered directly by the Rust binary — no Node.js, npm, or frontend build toolchains required.

---

## Key Features

### 🛡️ Hardware-Backed Passkeys (WebAuthn / FIDO2)
- Pure passwordless authentication using Touch ID, Face ID, YubiKeys, Windows Hello, or Android/iOS Passkeys.
- Enforced single-administrator model: eliminates unauthorized privilege escalation or accidental multi-admin registrations.
- State-of-the-art session security: tamper-proof encrypted cookies derived from your secret key using SHA-256.

### 🔒 Zero Public Attack Surface Registration
- Web registration routes are disabled by design.
- Admin registration is triggered exclusively on-demand via the CLI (`--register`).
- Generates a cryptographically random, one-time registration token (5-minute TTL) alongside an interactive **ASCII QR code** directly in your terminal for instant mobile passkey enrollment over SSH.

### 🐳 Full Vaultwarden Container Lifecycle Management
- **Status & Uptime**: Real-time status detection (`running`, `stopped`, `restarting`).
- **One-Click Actions**: Start, stop, and restart with instant visual feedback.
- **Image Updates**: Pull the latest image tag directly from the registry without terminal intervention.
- **Seamless Container Rebuild**: Automatically pulls the newest image, stops and destroys the outdated container, and recreates it with identical port mappings, environment variables, volumes, and network settings.

### 📊 Real-Time Server & Container Monitoring
- HTMX live polling every 5 seconds.
- **Host Metrics**: Global CPU usage %, RAM utilization, and Total/Available disk space (offloaded to Tokio's blocking thread pool to prevent async stalls).
- **Container Metrics**: Instantaneous container CPU % (with delta caching for Linux cgroups v2) and exact memory usage in megabytes.

---

## Architecture & Code Quality

Komainu is engineered following **Hexagonal Architecture (Ports and Adapters)** and **SOLID principles**:

```
locales/                # Language definition files (JSON)
├── en.json             # English translations
└── pt-BR.json          # Portuguese (Brazil) translations
src/
├── domain/             # Core business models & traits (zero external deps)
│   ├── auth.rs         # Authentication credentials & passkey types
│   ├── container.rs    # Container status, lifecycle states, ports
│   └── system.rs       # System metrics domain models
├── application/        # Application services & business logic orchestrators
│   ├── auth_service.rs # WebAuthn challenge/registration orchestration & TTL cleanup
│   ├── container_service.rs # Container lifecycle orchestration
│   └── system_service.rs    # Resource metric aggregation
├── infrastructure/     # Concrete adapters & external drivers
│   ├── auth_store.rs   # JSON-backed single-admin credential store
│   ├── docker.rs       # Docker daemon communication via Bollard & CPU delta cache
│   ├── http/           # Axum web server, HTMX handlers, HTML templates
│   └── system.rs       # Sysinfo adapter with non-blocking disk inspection
├── cli.rs              # Command-line interface definitions (Clap)
├── config.rs           # Environment variable validation & security alerts
├── i18n.rs             # Internationalization loader & strongly-typed translations
└── main.rs             # Composition root & dependency injection
```

---

## Prerequisites

1. **Docker Engine**: Docker daemon running with access to the Unix socket (`/var/run/docker.sock`).
2. **Reverse Proxy (HTTPS)**: WebAuthn/Passkeys require a secure context (`https://` or `localhost`). You must expose Komainu through a reverse proxy (such as Caddy or Nginx) configured with valid SSL/TLS certificates.

---

## Configuration

Komainu is configured via environment variables. Create a `.env` file in the application directory:

```bash
cp .env.example .env
```

| Variable | Default | Description |
|---|---|---|
| `SERVER_PORT` | `3000` | Port on which Komainu's HTTP server listens. |
| `DOCKER_SOCKET` | `/var/run/docker.sock` | Path to the local Docker Unix socket. |
| `SESSION_SECRET` | *(Random 32+ bytes)* | High-entropy secret used for encrypting signed session cookies. |
| `VW_CONTAINER_NAME` | `vaultwarden` | Name of your running Vaultwarden Docker container. |
| `RP_ID` | `localhost` | WebAuthn Relying Party ID (your domain name, e.g., `komainu.example.com`). |
| `RP_ORIGIN` | `http://localhost:3000` | WebAuthn Relying Party Origin (e.g., `https://komainu.example.com`). |
| `CREDENTIALS_PATH` | `credentials.json` | File path where the admin passkey credential is saved. |
| `APP_LANG` | `pt-BR` | Interface & CLI language (`pt-BR`, `en`). |

> [!IMPORTANT]
> Always generate a secure `SESSION_SECRET` with at least 32 bytes of entropy:
> ```bash
> openssl rand -hex 32
> ```

---

## Building from Source

### 1. Requirements
- Rust 1.85+ (Edition 2024)
- [UPX](https://github.com/upx/upx) (optional, for maximum binary size reduction)

### 2. Standard Build
```bash
cargo build --release
```

### 3. Ultimate Binary Compression (~1.1 MB)
The `Cargo.toml` is pre-configured with size optimization flags:
```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Compress the compiled binary using UPX LZMA:
```bash
upx --best --lzma ./target/release/komainu
```

---

## Setup & Deployment Guide

### Step 1: Register Your Admin Passkey
To register your Passkey, run the binary with the `--register` flag over your secure SSH connection:

```bash
./target/release/komainu --register
```

Output:
```
================================================================================
  KOMAINU - REGISTRO DE PASSKEY ADMINISTRADOR
================================================================================

Acesse o link abaixo em seu navegador com suporte a Passkeys:
  http://localhost:3000/register?token=550e8400-e29b-41d4-a716-446655440000

Ou aponte a camera do seu celular para o QR Code abaixo:

  █████████████████████████████████
  █████████████████████████████████
  ... (QR Code displayed in terminal) ...
  █████████████████████████████████
  █████████████████████████████████

Nota: O token expira em 5 minutos e só pode ser utilizado uma única vez.
================================================================================
```

Open the link or scan the QR Code on your device (Touch ID / Face ID / YubiKey) to enroll your passkey. Once completed, registration is sealed.

*(Optional)* To re-register or replace an existing admin key:
```bash
./target/release/komainu --register --force
```

### Step 2: Start Komainu
Start the daemon:
```bash
./target/release/komainu
```

Access the dashboard through your configured reverse proxy, click **"Entrar com Passkey"**, and complete biometric authentication.

---

## Production Deployment Examples

### Systemd Service (`/etc/systemd/system/komainu.service`)
```ini
[Unit]
Description=Komainu - Vaultwarden Management Guardian
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/komainu
ExecStart=/opt/komainu/komainu
Restart=always
RestartSec=5
EnvironmentFile=/opt/komainu/.env

[Install]
WantedBy=multi-user.target
```

Enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now komainu
```

### Caddy Reverse Proxy (`Caddyfile`)
```caddy
komainu.yourdomain.com {
    reverse_proxy 127.0.0.1:3000
}
```

### Nginx Reverse Proxy
```nginx
server {
    listen 443 ssl http2;
    server_name komainu.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/komainu.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/komainu.yourdomain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
    }
}
```

---

## Testing

Run the full automated test suite:
```bash
cargo test
```

---

## License

This project is licensed under the [MIT License](LICENSE).

