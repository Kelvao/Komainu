// Arquitetura de Design System e CSS para o Komainu
// Macroestrutura: Workbench Utilitário
// Paleta: Crimson Dark (Obsidiana com subtons Carmesim / Ruby)
// Semântica de Ações: Cores distintas e preservadas para Iniciar, Parar, Reiniciar, Atualizar e Reconstruir

pub const KOMAINU_THEME_CSS: &str = include_str!("theme.css");

pub fn render_head(page_title: &str, extra_head: &str) -> String {
    format!(
        r##"<meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{page_title}</title>
    <link rel="icon" type="image/svg+xml" href="/assets/favicon.svg">
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&family=Space+Grotesk:wght@500;600;700&display=swap" rel="stylesheet">
    <link rel="stylesheet" href="/assets/theme.css">
    {extra_head}"##,
        page_title = page_title,
        extra_head = extra_head
    )
}

