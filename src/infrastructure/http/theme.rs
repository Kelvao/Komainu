// Arquitetura de Design System e CSS para o Komainu
// Macroestrutura: Workbench Utilitário
// Paleta: Crimson Dark (Obsidiana com subtons Carmesim / Ruby)
// Semântica de Ações: Cores distintas e preservadas para Iniciar, Parar, Reiniciar, Atualizar e Reconstruir

pub const KOMAINU_THEME_CSS: &str = r##"/* Hallmark · macrostructure: Workbench · genre: atmospheric · theme: Crimson Dark · anchor hue: ruby-rose */
:root {
  --bg-canvas: #090507;
  --bg-surface: #12090d;
  --bg-surface-elevated: #180d13;
  --bg-surface-inset: #090507;
  --border-subtle: rgba(255, 255, 255, 0.08);
  --border-subtle-hover: rgba(255, 255, 255, 0.16);
  --border-crimson: rgba(244, 63, 94, 0.18);
  --border-crimson-hover: rgba(244, 63, 94, 0.40);
  --accent-crimson: #e11d48;
  --accent-crimson-light: #fb7185;
  --accent-crimson-glow: rgba(225, 29, 72, 0.22);
}

body {
  background-color: var(--bg-canvas);
  background-image: 
    radial-gradient(circle at 50% -10%, rgba(225, 29, 72, 0.10) 0%, transparent 65%),
    radial-gradient(rgba(244, 63, 94, 0.035) 1px, transparent 1px);
  background-size: 100% 100%, 24px 24px;
}

/* Foco acessível uniforme em tom carmesim */
a:focus-visible, button:focus-visible, input:focus-visible {
  outline: 2px solid #f43f5e;
  outline-offset: 2px;
}

/* Transições suaves para HTMX */
.htmx-swapping {
  opacity: 0.85;
  transition: opacity 120ms ease-out;
}

/* Scrollbar estilizada */
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
::-webkit-scrollbar-track {
  background: var(--bg-canvas);
}
::-webkit-scrollbar-thumb {
  background: rgba(244, 63, 94, 0.2);
  border-radius: 3px;
}
::-webkit-scrollbar-thumb:hover {
  background: rgba(244, 63, 94, 0.4);
}
"##;

pub fn render_head(page_title: &str, extra_head: &str) -> String {
    format!(
        r##"<meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>{page_title}</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&family=Space+Grotesk:wght@500;600;700&display=swap" rel="stylesheet">
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
      tailwind.config = {{
        darkMode: 'class',
        theme: {{
          extend: {{
            fontFamily: {{
              display: ['"Space Grotesk"', 'sans-serif'],
              sans: ['"Inter"', 'sans-serif'],
              mono: ['"JetBrains Mono"', 'monospace'],
            }},
            colors: {{
              crimson: {{
                50: '#fff1f2',
                100: '#ffe4e6',
                200: '#fecdd3',
                300: '#fda4af',
                400: '#fb7185',
                500: '#f43f5e',
                600: '#e11d48',
                700: '#be123c',
                800: '#9f1239',
                900: '#881337',
                950: '#4c0519',
              }}
            }}
          }}
        }}
      }};
    </script>
    <style>
{KOMAINU_THEME_CSS}
    </style>
    {extra_head}"##,
        page_title = page_title,
        extra_head = extra_head
    )
}

