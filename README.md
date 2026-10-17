# Komainu (狛犬)

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.85%2B-orange?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Pure_Rust-Zero_C%2FOpenSSL_Deps-success?style=for-the-badge" alt="Pure Rust" />
  <img src="https://img.shields.io/badge/MUSL-100%25_Static_Ready-purple?style=for-the-badge" alt="MUSL Static Ready" />
  <img src="https://img.shields.io/badge/Binary_Size-~2.0_MB_(compressed)-success?style=for-the-badge" alt="Binary Size" />
  <img src="https://img.shields.io/badge/Auth-WebAuthn_%2F_Passkeys-rose?style=for-the-badge&logo=fido" alt="Passkeys" />
  <img src="https://img.shields.io/badge/Backups-AES--256--GCM%20%2B%20pCloud-emerald?style=for-the-badge" alt="Encrypted Backups" />
  <img src="https://img.shields.io/badge/Frontend-HTMX_%2B_Tailwind-informational?style=for-the-badge" alt="HTMX" />
  <img src="https://img.shields.io/badge/License-MIT-lightgrey?style=for-the-badge" alt="License" />
</p>

> **Komainu (狛犬)** are lion-dog guardian statues traditionally positioned at the entrance of Japanese Shinto shrines to ward off evil spirits.
> In that same spirit, **Komainu** serves as a lightweight, secure, and battle-tested single-binary guardian for your self-hosted **[Vaultwarden](https://github.com/dani-garcia/vaultwarden)** Docker instances.

---

## Overview

Self-hosting Vaultwarden gives you complete control over your passwords and secrets, but managing the underlying container typically means either relying on raw SSH commands or deploying heavyweight container management platforms (like Portainer), which introduce unwanted complexity, memory overhead, and security exposure.

**Komainu** solves this with a hyper-focused, pure Rust single-binary management suite:
- **100% Pure Rust with Zero C/OpenSSL Dependencies**: Uses pure Rust cryptography (`passkey-auth`, `ring`, `aes-gcm`). No `libssl-dev`, no `openssl-sys`, and no C toolchains required.
- **Effortless Static MUSL Compilation**: Builds out of the box for `x86_64-unknown-linux-musl` for deployment on Alpine, scratch containers, or any Linux distribution without glibc or dynamic linking headaches.
- **Ultra-compact Footprint**: Compiles into a single standalone binary (~2.0 MB with UPX compression).
- **Passwordless Security**: Hardware-backed Passkey (WebAuthn/FIDO2) authentication with enforced single-admin model.
- **Cross-Device Smartphone Key**: Log in on any PC by scanning a dynamic on-screen QR code with your smartphone camera and verifying biometrics on your phone.
- **Automated & Encrypted Off-Site Backups**: Non-locking hot backup via `vaultwarden backup`, packaging attachments, sends, env, and RSA keys from `vw-data/`, encrypted client-side with AES-256-GCM, and uploaded to pCloud on a configurable cron schedule with remote cloud file browsing and one-click decrypted downloads.
- **Zero Public Registration**: No web registration forms. The admin Passkey is registered strictly via CLI over SSH with ASCII terminal QR code support.
- **Live Dual-Layer Telemetry**: Real-time 2-second polling for both the host machine and the isolated container.
- **Mobile-Responsive Industrial UI**: Built with Axum + HTMX + Tailwind CSS embedded directly in the binary — zero Node.js, npm, or frontend build toolchains. Fully responsive across desktop and mobile screens (from 320px up).

---

## Key Features

### 🛡️ Hardware-Backed Passkeys & Cross-Device QR Login
- **Pure Rust WebAuthn (`passkey-auth`)**: Built without OpenSSL or C dependencies, providing memory safety and painless cross-compilation.
- **Local Passkey Mode**: One-click passwordless login using Touch ID, Face ID, Windows Hello, or hardware security keys (YubiKey).
- **Smartphone Key (QR Code Login)**: Log in securely on any desktop by scanning a dynamic QR code on screen with your smartphone camera. Authorize access with your phone's biometrics, and the PC logs in automatically in real time.
- **QR Session Fixation Defense**: Cryptographically binds the QR polling session to the initiator browser using signed cookies.
- **Enforced Single-Admin Model**: Eliminates privilege escalation and unauthorized multi-user registrations.
- **Tamper-Proof Sessions**: Signed HTTP-only session cookies derived with SHA-512 and authenticated with AES-GCM encryption. Auto-generates a secure persistent secret in `.session_secret` (0600 permissions) if not provided.
- **Dynamic Language Switcher**: Switch on the fly between English and Portuguese (`EN` / `PT`) via the navigation bar and login screen, persisted across sessions with signed cookies.

### 💾 Automated & Encrypted Off-Site Backups (pCloud)
- **Zero-Downtime Hot Snapshots**: Executes the native `vaultwarden backup` command inside the container to safely snapshot SQLite without locks, corruption risks, or container restarts.
- **Complete Secret & Data Packaging**: Bundles the database snapshot alongside `attachments/`, `sends/`, `vaultwarden.env`, `rsa_key*`, and `templates/` directly from the configured `vw-data/` path.
- **Client-Side AES-256-GCM Encryption**: The bundled `.zip` archive is encrypted client-side with AES-256-GCM using PBKDF2 (100,000 iterations) before any network transmission.
- **Direct pCloud Cloud Storage Sync & OAuth 2.0**: Direct API streaming to pCloud (supporting EU and US data regions, and custom folder targets). Connects securely through standard OAuth 2.0 with your pCloud app credentials.
- **Remote Cloud Archive Explorer**: Lists backup archives directly from your chosen pCloud destination folder.
- **One-Click Decrypted Downloads**: Download any remote encrypted backup archive with on-the-fly streaming AES-256-GCM decryption, serving a clean, unencrypted `.zip` archive ready to inspect or restore.
- **Configurable Cron Scheduler**: Integrated background cron daemon supporting standard cron syntax with quick presets (Daily at 03:00, Every 12h, Weekly) and live upcoming run calculation.
- **On-Demand Execution**: Instant tactile "Fazer Backup Agora" button with real-time feedback and execution status.
- **Audit & History Table**: Searchable history tracking execution date, archive filename, size, cloud status badge, and direct decrypted download.

### 🔒 Enterprise-Grade Security & Hardening
- **Zero Public Attack Surface Registration**: Admin registration is triggered exclusively on-demand via the CLI (`--register`) with a one-time token (5-minute TTL) and interactive terminal ASCII QR code.
- **Strict CSRF Defense**: Validates both `Sec-Fetch-Site` and `Origin` headers on all mutating endpoints (`POST`).
- **Anti-Spoofing Client IP Resolution**: Rejects spoofed `X-Forwarded-For` or `X-Real-IP` headers from untrusted networks, binding rate limiters directly to legitimate peer addresses.
- **In-Memory Rate Limiting with Hard Caps**: Protects authentication endpoints against abuse while capping tracking tables to 5,000 entries with automatic LRU pruning.
- **Bounded Challenge Memory**: QR challenges and registration tokens are strictly limited to 100 entries with automatic TTL eviction, completely preventing memory saturation attacks.
- **Complete Security Headers**: Automated middleware injecting strict Content-Security-Policy (CSP), `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy`, `Strict-Transport-Security`, and `Permissions-Policy`.

### 🐳 Full Vaultwarden Container Lifecycle Management
- **Status & Uptime**: Real-time container state detection (`running`, `stopped`, `restarting`).
- **One-Click Controls**: Start, stop, and restart with tactile feedback.
- **Image Updates**: Pull the newest image tag directly from Docker Hub or your registry.
- **Seamless Container Rebuild**: Automatically pulls the latest image, destroys the outdated container, and recreates it with identical port mappings, environment variables, volumes, and network settings.

### 📊 Real-Time Dual-Layer Telemetry (2-Second Polling)
- HTMX live polling every **2 seconds** without full page reloads.
- **Container Telemetry**: Isolated CPU usage % and exact RAM consumption in MB (with cgroups v2 delta calculation).
- **Host Hardware Matrix**: Dedicated Bento grid reporting CPU Core load %, Physical RAM (Used/Total/Occupied %), and Disk Storage (Used/Total/Occupied %).
- **Low-Memory Warning**: Automatically detects absent swapfiles on minimal cloud instances (e.g., 1 GB RAM VPS) and suggests safe one-line swap configuration to prevent Linux OOM Killer crashes.

---

## Architecture & Code Quality

Komainu is engineered following **Hexagonal Architecture (Ports and Adapters)**, **SOLID principles**, and strict clean code standards:

```
locales/                        # Language definition files (JSON)
├── en.json                     # English translations
└── pt-BR.json                  # Portuguese (Brazil) translations
src/
├── domain/                     # Core business models & traits (zero external deps)
│   ├── auth.rs                 # Authentication credentials & passkey types
│   ├── backup.rs               # Backup configuration, records, regions, and crypto models
│   ├── container.rs            # Container status, lifecycle states, ports
│   └── system.rs               # System metrics domain models
├── application/                # Application services & orchestrators
│   ├── auth_service.rs         # Passkey challenges, registration tokens & QR orchestration
│   └── backup_service/         # Modular backup service
│       ├── archiver.rs         # Hot snapshot, zip packaging & AES-GCM file encryption
│       ├── mod.rs              # Backup service facade
│       ├── scheduler.rs        # Cron schedule calculation & periodic runner
│       └── service.rs          # Cloud sync, remote listing & decrypted download streaming
├── infrastructure/             # Concrete adapters & external drivers
│   ├── auth_store.rs           # JSON-backed single-admin credential store
│   ├── backup_store.rs         # JSON-backed backup configuration and history store
│   ├── docker.rs               # Docker daemon communication via Bollard & CPU delta cache
│   ├── http/                   # Axum web server, middleware, routes, views & handlers
│   │   ├── handlers/           # Modular route handlers (auth, container, backups, metrics)
│   │   ├── views/              # HTMX/Tailwind templates (navbar, dashboard, backups, auth)
│   │   ├── middleware.rs       # Security headers, rate limiting & CSRF verification
│   │   ├── rate_limit.rs       # In-memory bounded rate limiter
│   │   ├── routes.rs           # Centralized router definition
│   │   ├── session.rs          # Authenticated session extraction & cookies
│   │   └── theme.rs            # Centralized CSS head tags, fonts, and dark palette
│   ├── pcloud.rs               # pCloud REST API client (EU/US regions, upload/download/list)
│   └── system.rs               # Sysinfo adapter with non-blocking metrics collection
├── cli.rs                      # Command-line interface definitions (Clap)
├── config.rs                   # Environment variable validation & auto secret persistence
├── i18n.rs                     # Internationalization loader & strongly-typed translations
├── lib.rs                      # Shared library root
└── main.rs                     # Composition root, background cron daemon & dependency injection
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
| `SERVER_HOST` | `127.0.0.1` | Network interface to bind HTTP listener to (`127.0.0.1` for local proxy, `0.0.0.0` for Docker). |
| `SERVER_PORT` | `3000` | Port on which Komainu's HTTP server listens. |
| `DOCKER_SOCKET` | `/var/run/docker.sock` | Path to the local Docker Unix socket. |
| `SESSION_SECRET` | *(Auto-generated)* | 32+ bytes secret for encrypting session cookies. Auto-saved to `.session_secret` if omitted. |
| `VW_CONTAINER_NAME` | `vaultwarden` | Name of your running Vaultwarden Docker container. |
| `RP_ID` | `localhost` | WebAuthn Relying Party ID (your domain name, e.g., `komainu.example.com`). |
| `RP_ORIGIN` | `http://localhost:3000` | WebAuthn Relying Party Origin (e.g., `https://komainu.example.com`). |
| `CREDENTIALS_PATH` | `credentials.json` | File path where the admin passkey credential is saved. |
| `APP_LANG` | `en` | Default interface & CLI language (`en`, `pt-BR`). |
| `VW_DATA_PATH` | `vw-data` | Host path to the Vaultwarden data directory (contains database, attachments, keys). |
| `BACKUP_CONFIG_PATH` | `backup_config.json` | Path where backup settings, cron configuration, and history are stored. |
| `PCLOUD_CLIENT_ID` | `""` | Optional default pCloud OAuth Application Client ID (App Key). |
| `PCLOUD_CLIENT_SECRET` | `""` | Optional default pCloud OAuth Application Client Secret (App Secret). |

### `.env.example`

```dotenv
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
DOCKER_SOCKET=/var/run/docker.sock
# SESSION_SECRET= # Optional: Auto-generated and persisted to .session_secret (chmod 0600) if unset
VW_CONTAINER_NAME=vaultwarden
RP_ID=localhost
RP_ORIGIN=http://localhost:3000
CREDENTIALS_PATH=credentials.json
APP_LANG=en # Options: en, pt-BR
VW_DATA_PATH=vw-data # Path to Vaultwarden data volume on host
BACKUP_CONFIG_PATH=backup_config.json # Path to store backup configuration and history
PCLOUD_CLIENT_ID= # Optional default pCloud App Key
PCLOUD_CLIENT_SECRET= # Optional default pCloud App Secret
```

---

## Building from Source

### 1. Requirements
- Rust 1.85+ (Edition 2024)
- [UPX](https://github.com/upx/upx) (optional, for maximum binary compression)

### 2. Standard Build
```bash
cargo build --release
```

### 3. Static MUSL Build (100% Standalone, Zero Dynamic Dependencies)
Because Komainu is pure Rust with zero OpenSSL or C-sys dependencies, static cross-compilation with MUSL works cleanly without needing any C development packages:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### 4. Maximum Binary Compression (~2.0 MB)
The `Cargo.toml` is pre-configured with binary size optimization:
```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Optionally compress the binary with UPX:
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

█▀▀▀▀▀█ ▄▄▄ ▀▀▀█ █    █▀▀▀▀▀█
█ ███ █ ███ █ █▄▀▄▀▄█ █ ███ █
█ ▀▀▀ █  ▀▄▀███▀▄ █▀█ █ ▀▀▀ █
▀▀▀▀▀▀▀ █▄▀ ▀▄▀ ▀ ▀ █ ▀▀▀▀▀▀▀
▀▄ ▄ ▄▀ ██▄▀  █▀ █▄▄ █▀▄▄▀██ 
▀▄ ▀▀▄▀██▄▀█  ▄▄▄▀ ▄▀▄  ▀▄   
▄ ▄█ ▄▀▀  ▄ ▄██ ▀▄█ █ ██▄  ▄█
▄█▄▄▄▄▀▄█ ▄█▀██▀▀▀ ▄▀▀ ▄█▀█ ▄
▄█ ▄▀▄▀ ▀▄ ██ ▀██▄▄█ ▀▄█▄▀█▄▄
█▀█ ▀ ▀▀ ▄█  ▀███ ▀▀▄ ▄ ▀▀  ▀
▀ ▀ ▀▀▀ ▄▄█▄▀▄ ▄ █▄▀█▀▀▀█▄▀▀▀
█▀▀▀▀▀█   ▄█▀█▀▀▀▀ ██ ▀ █▀▀▄ 
█ ███ █  █▀▀▀▀ ▄█▄▄███▀▀▀█▄▀█
█ ▀▀▀ █  █ █  ▀▀█  ▀▀▀██▀██▀▄
▀▀▀▀▀▀▀ ▀▀▀ ▀▀▀▀ ▀ ▀  ▀▀▀ ▀

Nota: O token expira em 5 minutos e só pode ser utilizado uma única vez.
===============================================================================
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
