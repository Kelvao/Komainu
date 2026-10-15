use clap::Parser;
use komainu::application::auth_service::AuthService;
use komainu::application::backup_service::BackupService;
use komainu::application::container_service::ContainerService;
use komainu::application::system_service::SystemService;
use komainu::cli::Cli;
use komainu::config::Config;
use komainu::infrastructure::auth_store::FileCredentialStore;
use komainu::infrastructure::backup_store::FileBackupStore;
use komainu::infrastructure::docker::DockerAdapter;
use komainu::infrastructure::http::{run_server, AppState};
use komainu::infrastructure::pcloud::HttpPCloudClient;
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
    let docker_adapter = Arc::new(DockerAdapter::new(&config.docker_socket));
    let container_service = Arc::new(ContainerService::new(
        docker_adapter.clone(),
        config.vw_container_name.clone(),
    ));

    let backup_store = Arc::new(FileBackupStore::new(&config.backup_config_path));
    let pcloud_client = Arc::new(HttpPCloudClient::new());
    let backup_service = Arc::new(BackupService::new(
        docker_adapter.clone(),
        backup_store,
        pcloud_client,
        config.vw_container_name.clone(),
        &config.vw_data_path,
    ));

    // Iniciar loop de cronjobs agendados em segundo plano
    backup_service.clone().start_cron_loop();

    let system_adapter = SysinfoAdapter::new();
    let system_service = Arc::new(SystemService::new(Arc::new(system_adapter)));

    let cookie_key = komainu::infrastructure::http::derive_cookie_key(&config.session_secret);
    let i18n = Arc::new(komainu::i18n::I18n::load(&config.app_lang));

    let state = AppState {
        container_service,
        system_service,
        auth_service: auth_service.clone(),
        backup_service,
        cookie_key,
        i18n: i18n.clone(),
        rp_origin: config.rp_origin.clone(),
        pcloud_client_id: config.pcloud_client_id.clone(),
        pcloud_client_secret: config.pcloud_client_secret.clone(),
    };

    if args.register {
        if auth_service.has_admin().await && !args.force {
            eprintln!("\n========================================================");
            eprintln!("        {}", i18n.cli.reg_blocked_title);
            eprintln!("========================================================");
            eprintln!("{}", i18n.cli.reg_blocked_admin_exists);
            eprintln!("{}", i18n.cli.reg_blocked_single_admin);
            eprintln!("{}", i18n.cli.reg_blocked_force_hint);
            eprintln!("========================================================\n");
            return Ok(());
        }

        let token = Uuid::new_v4().to_string();
        auth_service.set_registration_token(token.clone()).await;

        let reg_url = format!("{}/register?token={}", config.rp_origin.trim_end_matches('/'), token);

        println!("\n========================================================");
        println!("           {}", i18n.cli.reg_title);
        println!("========================================================");
        println!("{}", i18n.cli.reg_scan_prompt);

        if let Ok(code) = qrcode::QrCode::new(reg_url.as_bytes()) {
            let qr_string = code.render::<qrcode::render::unicode::Dense1x2>().build();
            println!("{qr_string}\n");
        }

        println!("{}\n{reg_url}\n", i18n.cli.reg_direct_link);
        println!("{}", i18n.cli.reg_single_use);
        println!("{}", i18n.cli.reg_ctrl_c);
        println!("========================================================\n");

        run_server(&config.server_host, config.server_port, state).await?;
        return Ok(());
    }

    if !auth_service.has_admin().await {
        println!("\n{}", i18n.cli.warn_no_admin_title);
        println!("{}\n", i18n.cli.warn_no_admin_help);
    }

    info!(
        "Iniciando Komainu para o container '{}' em {}:{}",
        config.vw_container_name, config.server_host, config.server_port
    );
    run_server(&config.server_host, config.server_port, state).await?;

    Ok(())
}
