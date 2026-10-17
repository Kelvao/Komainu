#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActiveNav {
    Home,
    Backups,
}

pub fn render_navbar(
    i18n: &crate::i18n::Translations,
    active: ActiveNav,
    current_path: &str,
) -> String {
    let (next_lang, lang_btn_label, lang_title) = if i18n.lang == "en" {
        ("pt-BR", "PT", "Mudar para Português")
    } else {
        ("en", "EN", "Switch to English")
    };

    let home_url = "/";
    let backups_url = "/backups";

    let app_version = concat!("v", env!("CARGO_PKG_VERSION"));

    let home_classes = if active == ActiveNav::Home {
        "bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm"
    } else {
        "text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent"
    };

    let backups_classes = if active == ActiveNav::Backups {
        "bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm"
    } else {
        "text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent"
    };

    let lang_redirect = current_path;

    format!(
        r##"<header class="border-b border-rose-500/15 bg-canvas/90 backdrop-blur-md sticky top-0 z-50">
        <div class="max-w-6xl mx-auto px-3 sm:px-4 py-2 sm:py-3 flex items-center justify-between gap-2 sm:gap-3">
            <div class="flex items-center gap-2 sm:gap-6">
                <a href="{home_url}" class="flex items-center gap-2 sm:gap-3 group">
                    <div class="w-7 h-7 sm:w-9 sm:h-9 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center p-1 sm:p-1.5 text-rose-400 shrink-0 group-hover:border-rose-500/50 group-hover:text-rose-300 transition-colors">
                        {logo_svg}
                    </div>
                    <div>
                        <div class="flex items-center gap-1.5 sm:gap-2">
                            <h1 class="text-xs sm:text-base font-display font-bold tracking-tight text-white">{app_title}</h1>
                            <span class="hidden md:inline-block px-1.5 py-0.2 rounded text-[10px] font-mono uppercase bg-white/[0.05] text-slate-400 border border-white/[0.08]">{app_version}</span>
                        </div>
                        <p class="hidden sm:block text-[11px] text-slate-400 font-normal">{app_subtitle}</p>
                    </div>
                </a>

                <!-- Navigation Tabs -->
                <nav class="flex items-center gap-0.5 sm:gap-1.5 p-0.5 sm:p-1 bg-surface-elevated/80 rounded-lg border border-white/[0.06]">
                    <a href="{home_url}" class="px-2 sm:px-3 py-1 rounded-md text-xs font-mono font-medium transition-colors duration-150 {home_classes}">
                        {nav_home}
                    </a>
                    <a href="{backups_url}" class="px-2 sm:px-3 py-1 rounded-md text-xs font-mono font-medium transition-colors duration-150 {backups_classes}">
                        {nav_backups}
                    </a>
                </nav>
            </div>

            <div class="flex items-center gap-1.5 sm:gap-3 shrink-0">
                <span class="inline-flex items-center gap-1.5 px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-medium bg-rose-500/10 text-rose-300 border border-rose-500/25" title="{active_monitoring}">
                    <span class="w-1.5 h-1.5 rounded-full bg-rose-400 animate-pulse"></span>
                    <span class="hidden sm:inline">{active_monitoring}</span>
                </span>

                <a href="/set-lang?lang={next_lang}&redirect={lang_redirect}" class="px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-semibold text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1 sm:gap-1.5 border border-white/[0.08] hover:border-rose-500/40" title="{lang_title}">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-.778.099-1.533.284-2.253"/></svg>
                    <span>{lang_btn_label}</span>
                </a>

                <form method="POST" action="/logout" class="inline m-0 p-0">
                    <button type="submit" class="px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-medium text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1.5 border border-white/[0.08] hover:border-rose-500/40 cursor-pointer" title="{logout}" aria-label="{logout}">
                        <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"/></svg>
                        <span class="hidden sm:inline">{logout}</span>
                    </button>
                </form>
            </div>
        </div>
    </header>"##,
        home_url = home_url,
        backups_url = backups_url,
        app_title = i18n.common.app_title,
        app_subtitle = i18n.common.app_subtitle,
        home_classes = home_classes,
        backups_classes = backups_classes,
        nav_home = i18n.dashboard.nav_home,
        nav_backups = i18n.dashboard.nav_backups,
        active_monitoring = i18n.common.active_monitoring,
        next_lang = next_lang,
        lang_redirect = lang_redirect,
        lang_btn_label = lang_btn_label,
        lang_title = lang_title,
        logout = i18n.common.logout,
        logo_svg = crate::infrastructure::http::icon::KOMAINU_ICON_SVG,
    )
}

