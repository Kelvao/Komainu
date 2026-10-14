use clap::Parser;
use komainu::application::auth_service::AuthService;
use komainu::application::container_service::ContainerService;
use komainu::application::system_service::SystemService;
use komainu::cli::Cli;
use komainu::config::Config;
use komainu::infrastructure::auth_store::FileCredentialStore;
use komainu::infrastructure::docker::DockerAdapter;
use komainu::infrastructure::http::{run_server, AppState};
use komainu::infrastructure::system::SysinfoAdapter;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    let config = Config::from_env()?;

    info!("Inicializando armazenamento de credenciais em '{}'...", config.credentials_path);
    let store = Arc::new(FileCredentialStore::new(&config.credentials_path));

    let auth_service = Arc::new(AuthService::new(
        &config.rp_id,
        &config.rp_origin,
        store.clone(),
    )?);

    info!("Configurando adaptador Docker em '{}'...", config.docker_socket);
    let docker_adapter = DockerAdapter::new(&config.docker_socket);
    let container_service = Arc::new(ContainerService::new(
        Arc::new(docker_adapter),
        config.vw_container_name.clone(),
    ));

    let system_adapter = SysinfoAdapter::new();
    let system_service = Arc::new(SystemService::new(Arc::new(system_adapter)));

    let cookie_key = komainu::infrastructure::http::derive_cookie_key(&config.session_secret);

    let state = AppState {
        container_service,
        system_service,
        auth_service: auth_service.clone(),
        cookie_key,
    };

    if args.register {
        if auth_service.has_admin().await && !args.force {
            eprintln!("\n========================================================");
            eprintln!("        KOMAINU - REGISTRO BLOQUEADO                   ");
            eprintln!("========================================================");
            eprintln!("Já existe uma Passkey de administrador cadastrada!");
            eprintln!("O Komainu é restrito a um único administrador.");
            eprintln!("Para substituir a Passkey existente, execute com --force:");
            eprintln!("  ./komainu --register --force\n");
            eprintln!("========================================================\n");
            return Ok(());
        }

        let token = Uuid::new_v4().to_string();
        auth_service.set_registration_token(token.clone()).await;

        let reg_url = format!("{}/register?token={}", config.rp_origin, token);

        println!("\n========================================================");
        println!("           KOMAINU - REGISTRO DE PASSKEY                ");
        println!("========================================================");
        println!("Escaneie o QR Code com seu dispositivo móvel ou acesse o link:\n");

        if let Ok(code) = qrcode::QrCode::new(reg_url.as_bytes()) {
            let qr_string = code.render::<qrcode::render::unicode::Dense1x2>().build();
            println!("{qr_string}\n");
        }

        println!("Link direto:\n{reg_url}\n");
        println!("Este link é de uso único. Após o cadastro, a rota será fechada.");
        println!("Pressione Ctrl+C para encerrar após o cadastro.");
        println!("========================================================\n");

        run_server(config.server_port, state).await?;
        return Ok(());
    }

    if !auth_service.has_admin().await {
        println!("\n[AVISO] Nenhum administrador cadastrado!");
        println!("Execute './komainu --register' via terminal/SSH para cadastrar sua Passkey com segurança.\n");
    }

    info!(
        "Iniciando Komainu para o container '{}' na porta {}",
        config.vw_container_name, config.server_port
    );
    run_server(config.server_port, state).await?;

    Ok(())
}
