use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "komainu", about = "Vaultwarden Docker Manager in Rust")]
pub struct Cli {
    /// Inicia o fluxo seguro de registro da Passkey de administrador via terminal
    #[arg(long)]
    pub register: bool,
}

