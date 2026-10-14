use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "komainu", about = "Vaultwarden Docker Manager in Rust")]
pub struct Cli {
    /// Inicia o fluxo seguro de registro da Passkey de administrador via terminal
    #[arg(long)]
    pub register: bool,

    /// Força o recadastro do administrador substituindo a Passkey existente
    #[arg(long)]
    pub force: bool,
}
