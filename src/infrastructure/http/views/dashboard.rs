use crate::domain::container::ContainerState;
use crate::i18n::Translations;
use crate::infrastructure::http::AppState;
use super::navbar::{render_navbar, ActiveNav};
use crate::infrastructure::http::theme;

pub fn render_dashboard_page(i18n: &Translations, fragment: &str) -> String {
    let navbar = render_navbar(i18n, ActiveNav::Home, "/");
    let head_tags = theme::render_head(
        &i18n.dashboard.page_title,
        r#"<script src="/assets/htmx.js"></script>"#,
    );
    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex flex-col font-sans antialiased selection:bg-rose-600 selection:text-white">
    {navbar}

    <main class="flex-1 max-w-6xl w-full mx-auto px-3 sm:px-4 py-4 sm:py-8">
        <div id="dashboard-content" hx-get="/api/dashboard-fragment" hx-trigger="every 2s" hx-swap="innerHTML">
            {fragment}
        </div>
    </main>

    <footer class="border-t border-rose-500/10 py-4 text-center text-xs font-mono text-slate-500">
        {footer}
    </footer>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        navbar = navbar,
        fragment = fragment,
        footer = i18n.common.footer,
    )
}

pub async fn render_dashboard_fragment(
    state: &AppState,
    i18n: &Translations,
    action_error: Option<String>,
) -> String {
    let status_res = state.container_port.get_status(&state.target_container).await;
    let metrics = state.system_monitor.get_metrics().await;

    let error_banner = match action_error {
        Some(e) => format!(
            r##"<div id="action-error-banner" class="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-400 font-bold shrink-0">FAIL</span>
                    <span><strong>{}</strong> {}</span>
                </div>
                <button type="button" onclick="document.getElementById('action-error-banner').remove()" class="text-rose-400/70 hover:text-rose-200 transition p-1 rounded-md hover:bg-rose-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            i18n.common.operation_failed,
            super::html_escape(&e)
        ),
        None => String::new(),
    };

    let (status_badge, image_name, uptime_str, container_cpu_val, container_cpu_bar_pct, container_ram_val, container_ram_bar_pct) = match &status_res {
        Ok(status) => {
            let badge = match status.state {
                ContainerState::Running => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span> {}
                    </span>"##,
                        i18n.dashboard.status_running
                    )
                }
                ContainerState::Stopped => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-rose-500/10 text-rose-400 border border-rose-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-rose-400"></span> {}
                    </span>"##,
                        i18n.dashboard.status_stopped
                    )
                }
                ContainerState::Restarting => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-amber-400 animate-spin"></span> {}
                    </span>"##,
                        i18n.dashboard.status_restarting
                    )
                }
                _ => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-white/[0.05] text-slate-400 border border-white/[0.08]">
                        {}
                    </span>"##,
                        i18n.dashboard.status_unknown
                    )
                }
            };

            let cpu_val = status.cpu_usage_pct.map(|c| format!("{c:.1}%")).unwrap_or_else(|| "0.0%".to_string());
            let cpu_bar = status.cpu_usage_pct.unwrap_or(0.0).clamp(0.0, 100.0);

            let (ram_val, ram_bar) = match (status.memory_used_mb, status.memory_limit_mb) {
                (Some(used), Some(limit)) if limit > 0 && limit < metrics.ram_total_mb => {
                    let pct = (used as f32 / limit as f32) * 100.0;
                    (format!("{used} / {limit} MB ({pct:.1}%)"), pct.clamp(0.0, 100.0))
                }
                (Some(used), _) => {
                    let pct = if metrics.ram_total_mb > 0 {
                        (used as f32 / metrics.ram_total_mb as f32) * 100.0
                    } else {
                        0.0
                    };
                    (format!("{used} MB ({pct:.1}%)"), pct.clamp(0.0, 100.0))
                }
                (None, _) => ("0 MB (0.0%)".to_string(), 0.0),
            };

            (
                badge,
                super::html_escape(&status.image),
                status.uptime.clone().unwrap_or_else(|| "N/A".to_string()),
                cpu_val,
                cpu_bar,
                ram_val,
                ram_bar,
            )
        }
        Err(e) => (
            format!(
                r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-rose-500/10 text-rose-400 border border-rose-500/30">
                    {} {}
                </span>"##,
                i18n.common.operation_failed,
                super::html_escape(&e.to_string())
            ),
            "N/A".to_string(),
            "N/A".to_string(),
            "0.0%".to_string(),
            0.0,
            "0 MB (0.0%)".to_string(),
            0.0,
        ),
    };

    let is_running = matches!(status_res.as_ref().map(|s| &s.state), Ok(ContainerState::Running));

    let swap_alert = if metrics.swap_total_mb == 0 {
        if i18n.lang == "en" {
            r#"<div class="mt-4 p-4 rounded-xl border border-amber-500/30 bg-amber-500/10 text-amber-200 text-xs font-mono space-y-1">
                <div class="flex items-center gap-2 font-semibold text-amber-300">
                    <span>⚠️</span>
                    <span>Performance Warning: No Swapfile detected (0 MB Swap)</span>
                </div>
                <p class="text-slate-300">On low-memory instances (e.g. 1 GB RAM on Oracle Cloud), creating a 1-2 GB swapfile prevents Vaultwarden crashes from the Linux OOM Killer. Run in host terminal:</p>
                <code class="block bg-black/40 p-2 rounded border border-white/10 text-amber-200 select-all overflow-x-auto">sudo fallocate -l 1G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile</code>
            </div>"#
        } else {
            r#"<div class="mt-4 p-4 rounded-xl border border-amber-500/30 bg-amber-500/10 text-amber-200 text-xs font-mono space-y-1">
                <div class="flex items-center gap-2 font-semibold text-amber-300">
                    <span>⚠️</span>
                    <span>Aviso de Performance: Nenhum Swapfile detectado (0 MB Swap)</span>
                </div>
                <p class="text-slate-300">Em instâncias com pouca memória (ex: 1 GB RAM na Oracle Cloud), configurar 1 GB ou 2 GB de swap previne quedas do Vaultwarden pelo Linux OOM Killer. Execute no terminal:</p>
                <code class="block bg-black/40 p-2 rounded border border-white/10 text-amber-200 select-all overflow-x-auto">sudo fallocate -l 1G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile</code>
            </div>"#
        }
    } else {
        ""
    };

    format!(
        r##"<div class="space-y-6">
    {error_banner}

    <!-- SEÇÃO 1: CONTAINER VAULTWARDEN -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-4 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 sm:gap-4 pb-4 sm:pb-5 border-b border-white/[0.06]">
            <div>
                <div class="flex flex-wrap items-center gap-2 sm:gap-3">
                    <h2 class="text-lg sm:text-xl font-display font-bold tracking-tight text-white">{instance_title}</h2>
                    {status_badge}
                </div>
                <p class="text-xs text-slate-400 mt-1 font-mono break-all sm:break-normal">{image_prefix} <span class="text-slate-300 font-semibold">{image_name}</span> &bull; {started_prefix} <span class="text-slate-300 font-semibold">{uptime_str}</span></p>
            </div>
            
            <div class="grid grid-cols-2 sm:flex sm:flex-wrap items-center gap-1.5 sm:gap-2 p-1 sm:p-1.5 bg-canvas/90 rounded-lg border border-white/[0.06] w-full sm:w-auto">
                <button 
                    hx-post="/api/container/start" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_start}
                    class="px-2.5 sm:px-3 py-2 sm:py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center gap-1.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-30 disabled:pointer-events-none text-white border border-rose-400/30 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>
                    <span>{btn_start}</span>
                </button>
                <button 
                    hx-post="/api/container/stop" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_stop}
                    class="px-2.5 sm:px-3 py-2 sm:py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center gap-1.5 bg-surface-elevated hover:bg-rose-950/80 active:translate-y-px disabled:opacity-30 disabled:pointer-events-none text-rose-300 hover:text-white border border-rose-500/30 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M4.5 4.5A1.5 1.5 0 016 3h8a1.5 1.5 0 011.5 1.5v11a1.5 1.5 0 01-1.5 1.5H6a1.5 1.5 0 01-1.5-1.5v-11z" clip-rule="evenodd"/></svg>
                    <span>{btn_stop}</span>
                </button>
                <button 
                    hx-post="/api/container/restart" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-2.5 sm:px-3 py-2 sm:py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                    <span>{btn_restart}</span>
                </button>
                <button 
                    hx-post="/api/container/update" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-2.5 sm:px-3 py-2 sm:py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                    <span>{btn_update}</span>
                </button>
                <button 
                    hx-post="/api/container/rebuild" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    hx-confirm="{confirm_rebuild}"
                    class="col-span-2 sm:col-span-1 px-2.5 sm:px-3 py-2 sm:py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                    <span>{btn_rebuild}</span>
                </button>
            </div>
        </div>

        <!-- Consumo Exclusivo do Container -->
        <div class="mt-6 pt-5 border-t border-white/[0.06]">
            <div class="flex items-center gap-2 mb-3">
                <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                <h3 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{container_section}</h3>
                <div class="flex-1 h-px bg-white/[0.04]"></div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-6 pt-1">
                <!-- CPU Container -->
                <div class="flex flex-col justify-between">
                    <div class="flex items-center justify-between mb-2">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{container_cpu}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{container_cpu_val}</span>
                    </div>
                    <div class="w-full bg-surface-elevated h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {container_cpu_bar_pct:.1}%"></div>
                    </div>
                </div>

                <!-- RAM Container -->
                <div class="flex flex-col justify-between">
                    <div class="flex items-center justify-between mb-2">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{container_ram}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{container_ram_val}</span>
                    </div>
                    <div class="w-full bg-surface-elevated h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {container_ram_bar_pct:.1}%"></div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- SEÇÃO 2: CONSUMO GERAL DA MÁQUINA (HOST) -->
    <div class="space-y-3">
        <div class="flex items-center gap-2">
            <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
            <h3 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{host_section}</h3>
            <div class="flex-1 h-px bg-white/[0.04]"></div>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 sm:gap-4">
            <!-- Métrica CPU Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-4 sm:p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between gap-2 mb-2 sm:mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400 truncate">{cpu_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20 shrink-0">{cpu_usage:.1}%</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {cpu_usage:.1}%"></div>
                    </div>
                </div>
                <div class="mt-3 sm:mt-4 pt-2.5 sm:pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{cpu_usage:.1}%</span>
                </div>
            </div>

            <!-- Métrica RAM Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-4 sm:p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between gap-2 mb-2 sm:mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400 truncate">{ram_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20 shrink-0">{ram_used} / {ram_total} MB</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {ram_pct:.1}%"></div>
                    </div>
                </div>
                <div class="mt-3 sm:mt-4 pt-2.5 sm:pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{ram_pct:.1}% {occupied_suffix}</span>
                </div>
            </div>

            <!-- Métrica Disco Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-4 sm:p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between gap-2 mb-2 sm:mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400 truncate">{disk_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20 shrink-0">{disk_used} / {disk_total} GB</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {disk_pct:.1}%"></div>
                    </div>
                </div>
                <div class="mt-3 sm:mt-4 pt-2.5 sm:pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{disk_pct:.1}% {occupied_suffix}</span>
                </div>
            </div>
        </div>
        {swap_alert}
    </div>
</div>"##,
        error_banner = error_banner,
        instance_title = i18n.dashboard.instance_title,
        status_badge = status_badge,
        image_prefix = i18n.dashboard.image_prefix,
        image_name = image_name,
        started_prefix = i18n.dashboard.started_prefix,
        uptime_str = uptime_str,
        disabled_start = if is_running { "disabled" } else { "" },
        disabled_stop = if !is_running { "disabled" } else { "" },
        btn_start = i18n.dashboard.btn_start,
        btn_stop = i18n.dashboard.btn_stop,
        btn_restart = i18n.dashboard.btn_restart,
        btn_update = i18n.dashboard.btn_update,
        btn_rebuild = i18n.dashboard.btn_rebuild,
        confirm_rebuild = i18n.dashboard.confirm_rebuild,
        container_section = i18n.metrics.container_section,
        container_cpu = i18n.metrics.container_cpu,
        container_cpu_val = container_cpu_val,
        container_cpu_bar_pct = container_cpu_bar_pct,
        container_ram = i18n.metrics.container_ram,
        container_ram_val = container_ram_val,
        container_ram_bar_pct = container_ram_bar_pct,
        host_section = i18n.metrics.host_section,
        cpu_title = i18n.metrics.cpu_title,
        cpu_usage = metrics.cpu_usage_pct,
        ram_title = i18n.metrics.ram_title,
        ram_used = metrics.ram_used_mb,
        ram_total = metrics.ram_total_mb,
        ram_pct = if metrics.ram_total_mb > 0 { (metrics.ram_used_mb as f32 / metrics.ram_total_mb as f32) * 100.0 } else { 0.0 },
        disk_title = i18n.metrics.disk_title,
        disk_used = metrics.disk_used_gb,
        disk_total = metrics.disk_total_gb,
        disk_pct = if metrics.disk_total_gb > 0 { (metrics.disk_used_gb as f32 / metrics.disk_total_gb as f32) * 100.0 } else { 0.0 },
        host_label = i18n.metrics.host_label,
        occupied_suffix = i18n.metrics.occupied_suffix,
        swap_alert = swap_alert,
    )
}

