use std::str::FromStr;
use crate::i18n::Translations;
use crate::infrastructure::http::AppState;
use super::navbar::{render_navbar, ActiveNav};
use crate::infrastructure::http::theme;

pub fn render_backups_page(i18n: &Translations, fragment: &str) -> String {
    let navbar = render_navbar(i18n, ActiveNav::Backups, "/backups");
    let head_tags = theme::render_head(
        &format!("{} - Komainu", i18n.dashboard.nav_backups),
        r#"<script src="https://unpkg.com/htmx.org@2.0.4"></script>"#,
    );
    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex flex-col font-sans antialiased selection:bg-rose-600 selection:text-white">
    {navbar}

    <main class="flex-1 max-w-6xl w-full mx-auto px-4 py-6 sm:py-8">
        <div id="backups-content" hx-get="/api/backups/fragment" hx-trigger="backupCompleted from:body" hx-swap="innerHTML">
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

pub fn format_file_date(modified: &str, filename: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(modified) {
        return dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string();
    }
    if let Some(pos) = filename.find("backup_") {
        let rest = &filename[pos + 7..];
        if rest.len() >= 15 {
            let date_part = &rest[..15];
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_part, "%Y%m%d_%H%M%S") {
                if let Some(local_dt) = naive.and_local_timezone(chrono::Local).single() {
                    return local_dt.format("%d/%m/%Y %H:%M:%S").to_string();
                }
                return naive.format("%d/%m/%Y %H:%M:%S").to_string();
            }
        }
    }
    if !modified.is_empty() {
        modified.to_string()
    } else {
        "N/A".to_string()
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

pub async fn render_backups_fragment(
    state: &AppState,
    i18n: &Translations,
    action_msg: Option<(bool, String)>,
) -> String {
    let config = state.backup_service.get_config().await.unwrap_or_default();
    let history = state.backup_service.get_history().await.unwrap_or_default();
    let remote_files = state.backup_service.list_remote_backups().await.unwrap_or_default();
    let is_in_progress = state.backup_service.is_in_progress().await;
    let data_exists = state.backup_service.data_path_exists();
    let env_exists = state.backup_service.env_path_exists();

    let alert_banner = match action_msg {
        Some((true, msg)) => format!(
            r##"<div id="backup-action-alert" class="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-400 font-bold shrink-0">OK</span>
                    <span>{}</span>
                </div>
                <button type="button" onclick="document.getElementById('backup-action-alert').remove()" class="text-emerald-400/70 hover:text-emerald-200 transition p-1 rounded-md hover:bg-emerald-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            msg
        ),
        Some((false, msg)) => format!(
            r##"<div id="backup-action-alert" class="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-400 font-bold shrink-0">FAIL</span>
                    <span>{}</span>
                </div>
                <button type="button" onclick="document.getElementById('backup-action-alert').remove()" class="text-rose-400/70 hover:text-rose-200 transition p-1 rounded-md hover:bg-rose-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            msg
        ),
        None => String::new(),
    };

    let next_run_str = if config.enabled {
        let schedule = cron::Schedule::from_str(&config.cron_expression)
            .or_else(|_| cron::Schedule::from_str(&format!("0 {}", config.cron_expression.trim())));
        match schedule {
            Ok(s) => s
                .upcoming(chrono::Local)
                .next()
                .map(|dt| dt.format("%d/%m/%Y às %H:%M:%S").to_string())
                .unwrap_or_else(|| i18n.backups.no_scheduled_run.clone()),
            Err(_) => i18n.backups.no_scheduled_run.clone(),
        }
    } else {
        i18n.backups.no_scheduled_run.clone()
    };

    let status_badge = if config.enabled {
        format!(
            r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/25 flex items-center gap-1.5">
                <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
                {} &bull; {}
            </span>"##,
            i18n.backups.cron_enabled, config.cron_expression
        )
    } else {
        format!(
            r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-white/[0.05] text-slate-400 border border-white/[0.08] flex items-center gap-1.5">
                <span class="w-1.5 h-1.5 rounded-full bg-slate-500"></span>
                {}
            </span>"##,
            i18n.backups.cron_disabled
        )
    };

    let run_btn_disabled = if is_in_progress { "disabled" } else { "" };
    let run_btn_text = if is_in_progress {
        &i18n.backups.btn_running
    } else {
        &i18n.backups.btn_backup_now
    };

    let cron_checked = if config.enabled { "checked" } else { "" };
    let region_eu_selected = if config.pcloud_region == crate::domain::backup::PCloudRegion::EU { "selected" } else { "" };
    let region_us_selected = if config.pcloud_region == crate::domain::backup::PCloudRegion::US { "selected" } else { "" };

    let data_path_badge = if data_exists {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{}</span>"##, i18n.backups.path_found)
    } else {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-rose-500/10 text-rose-400 border border-rose-500/20">{}</span>"##, i18n.backups.path_missing)
    };

    let env_path_badge = if env_exists {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{}</span>"##, i18n.backups.path_found)
    } else {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-rose-500/10 text-rose-400 border border-rose-500/20">{}</span>"##, i18n.backups.path_missing)
    };

    let mut history_rows = String::new();
    if !remote_files.is_empty() {
        for file in remote_files {
            let dt_str = format_file_date(&file.modified, &file.name);
            let sz_str = format_bytes(file.size_bytes);
            let enc_name = crate::infrastructure::pcloud::urlencoding_path(&file.name);
            history_rows.push_str(&format!(
                r##"<tr class="border-b border-white/[0.04] hover:bg-white/[0.02] transition-colors text-xs font-mono">
                    <td class="py-3 px-3 text-slate-300 whitespace-nowrap">{dt_str}</td>
                    <td class="py-3 px-3 text-white font-medium break-all">{filename}</td>
                    <td class="py-3 px-3 text-slate-300 tabular-nums whitespace-nowrap">{sz_str}</td>
                    <td class="py-3 px-3 whitespace-nowrap">
                        <span class="px-2 py-0.5 rounded text-[11px] font-semibold border bg-emerald-500/10 text-emerald-400 border-emerald-500/25">{status_remote}</span>
                    </td>
                    <td class="py-3 px-3 text-right whitespace-nowrap">
                        <a href="/api/backups/download?fileid={fileid}&filename={enc_name}" download class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-rose-600/20 hover:bg-rose-600/30 text-rose-300 hover:text-white border border-rose-500/30 hover:border-rose-500/50 text-[11px] font-mono font-medium transition-colors shadow-sm">
                            <svg class="w-3.5 h-3.5 text-rose-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                            <span>{btn_download}</span>
                        </a>
                    </td>
                </tr>"##,
                dt_str = dt_str,
                filename = file.name,
                sz_str = sz_str,
                status_remote = i18n.backups.status_remote,
                fileid = file.fileid,
                enc_name = enc_name,
                btn_download = i18n.backups.btn_download_decrypted,
            ));
        }
    } else if !history.is_empty() {
        for rec in history.iter().take(20) {
            let dt_str = chrono::DateTime::from_timestamp(rec.timestamp as i64, 0)
                .map(|dt| dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            let (badge, badge_color) = match &rec.status {
                crate::domain::backup::BackupStatus::Success => (i18n.backups.status_success.as_str(), "bg-emerald-500/10 text-emerald-400 border-emerald-500/25"),
                crate::domain::backup::BackupStatus::Failed(e) => (e.as_str(), "bg-rose-500/10 text-rose-400 border-rose-500/25"),
            };
            let sz_str = format_bytes(rec.size_bytes);
            history_rows.push_str(&format!(
                r##"<tr class="border-b border-white/[0.04] hover:bg-white/[0.02] transition-colors text-xs font-mono">
                    <td class="py-3 px-3 text-slate-300 whitespace-nowrap">{dt_str}</td>
                    <td class="py-3 px-3 text-white font-medium break-all">{filename}</td>
                    <td class="py-3 px-3 text-slate-300 tabular-nums whitespace-nowrap">{sz_str}</td>
                    <td class="py-3 px-3 whitespace-nowrap">
                        <span class="px-2 py-0.5 rounded text-[11px] font-semibold border {badge_color}">{badge}</span>
                    </td>
                    <td class="py-3 px-3 text-right text-slate-500 whitespace-nowrap">-</td>
                </tr>"##,
                dt_str = dt_str,
                filename = rec.filename,
                sz_str = sz_str,
                badge = badge,
                badge_color = badge_color,
            ));
        }
    } else {
        history_rows.push_str(&format!(
            r##"<tr><td colspan="5" class="py-8 text-center text-xs text-slate-500 font-mono">{}</td></tr>"##,
            i18n.backups.history_empty
        ));
    }

    let enc_pass_ph = if config.encryption_password.is_empty() {
        i18n.backups.encryption_password_placeholder.as_str()
    } else {
        "••••••••••••"
    };

    let client_id_val = if !config.pcloud_client_id.is_empty() {
        &config.pcloud_client_id
    } else {
        &state.pcloud_client_id
    };

    let client_secret_ph = if config.pcloud_client_secret.is_empty() && state.pcloud_client_secret.is_empty() {
        i18n.backups.pcloud_client_secret_placeholder.as_str()
    } else {
        "••••••••••••"
    };

    let callback_url = format!("{}/api/backups/pcloud/callback", state.rp_origin.trim_end_matches('/'));

    let is_pcloud_connected = !config.pcloud_token.is_empty();
    let pcloud_status_card = if is_pcloud_connected {
        let region_str = match config.pcloud_region {
            crate::domain::backup::PCloudRegion::EU => "EU",
            crate::domain::backup::PCloudRegion::US => "US",
        };
        format!(
            r##"<div class="p-3.5 rounded-lg bg-emerald-500/10 border border-emerald-500/25 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-2.5">
                    <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
                    <div>
                        <p class="text-xs font-mono font-semibold text-emerald-300">{} ({})</p>
                        <p class="text-[11px] text-slate-400 font-sans">Token OAuth ativo e pronto para backups.</p>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    <button type="button" onclick="startPCloudOAuth()" class="px-3 py-1.5 rounded-md bg-white/[0.06] hover:bg-white/[0.1] text-xs font-mono text-slate-200 border border-white/[0.08] transition-colors flex items-center gap-1.5">
                        <span>🔄</span> <span>{}</span>
                    </button>
                    <button type="button" hx-post="/api/backups/pcloud/disconnect" hx-target="#backups-content" hx-swap="innerHTML" class="px-3 py-1.5 rounded-md bg-rose-500/10 hover:bg-rose-500/20 text-xs font-mono text-rose-300 border border-rose-500/20 transition-colors flex items-center gap-1.5">
                        <span>✕</span> <span>{}</span>
                    </button>
                </div>
            </div>"##,
            i18n.backups.pcloud_connected,
            region_str,
            i18n.backups.pcloud_reconnect_btn,
            i18n.backups.pcloud_disconnect_btn
        )
    } else {
        format!(
            r##"<div class="p-3.5 rounded-lg bg-white/[0.02] border border-white/[0.08] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-2.5">
                    <span class="w-2.5 h-2.5 rounded-full bg-slate-500"></span>
                    <div>
                        <p class="text-xs font-mono font-semibold text-slate-300">{}</p>
                        <p class="text-[11px] text-slate-400 font-sans">Faça login com seu app pCloud para vincular o acesso aos backups.</p>
                    </div>
                </div>
                <div>
                    <button type="button" onclick="startPCloudOAuth()" class="w-full sm:w-auto px-4 py-2 rounded-lg text-xs font-semibold font-sans bg-rose-600 hover:bg-rose-500 active:translate-y-px text-white shadow-md flex items-center justify-center gap-2 transition-all">
                        <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                        <span>{}</span>
                    </button>
                </div>
            </div>"##,
            i18n.backups.pcloud_not_connected,
            i18n.backups.pcloud_oauth_btn
        )
    };

    format!(
        r##"<div class="space-y-6">
    {alert_banner}

    <!-- CARD 1: STATUS DO AGENDAMENTO E EXECUÇÃO RÁPIDA -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div>
                <div class="flex items-center gap-3">
                    <h2 class="text-xl font-display font-bold tracking-tight text-white">{backups_title}</h2>
                    {status_badge}
                </div>
                <p class="text-xs text-slate-400 font-mono mt-1.5 flex items-center gap-2">
                    <span>{next_run_label}</span>
                    <strong class="text-rose-300">{next_run_val}</strong>
                </p>
            </div>

            <div>
                <button 
                    hx-post="/api/backups/run" 
                    hx-target="#backups-content" 
                    hx-swap="innerHTML"
                    {run_btn_disabled}
                    class="w-full sm:w-auto px-4 py-2.5 rounded-lg text-xs font-semibold font-sans transition-all duration-150 flex items-center justify-center gap-2 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30 shadow-md">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/></svg>
                    <span>{run_btn_text}</span>
                </button>
            </div>
        </div>
    </div>

    <!-- CARD 2: CONFIGURAÇÕES DA ROTINA, CRON E PCLOUD -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="mb-5 pb-4 border-b border-white/[0.06]">
            <h3 class="text-lg font-display font-bold tracking-tight text-white">{section_config}</h3>
            <p class="text-xs text-slate-400 mt-0.5">{backups_subtitle}</p>
        </div>

        <form hx-post="/api/backups/config" hx-target="#backups-content" hx-swap="innerHTML" class="space-y-6">
            <!-- Bloco 1: Cronjob -->
            <div class="space-y-3">
                <div class="flex items-center gap-2.5">
                    <input type="checkbox" id="cron-enabled-check" name="enabled" {cron_checked} class="w-4 h-4 rounded bg-canvas border-white/[0.1] text-rose-600 focus:ring-rose-500">
                    <label for="cron-enabled-check" class="text-xs font-mono font-semibold text-slate-200 cursor-pointer">{enable_cron}</label>
                </div>

                <div>
                    <label for="cron-input" class="block text-xs font-mono text-slate-400 mb-1.5">{cron_expression_label}</label>
                    <input type="text" id="cron-input" name="cron_expression" value="{cron_val}" placeholder="0 3 * * *" class="w-full sm:max-w-md px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    <p class="text-[11px] text-slate-500 font-mono mt-1">{cron_hint}</p>

                    <!-- Presets rápidos -->
                    <div class="flex flex-wrap gap-1.5 mt-2">
                        <button type="button" onclick="document.getElementById('cron-input').value='0 3 * * *'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_daily}</button>
                        <button type="button" onclick="document.getElementById('cron-input').value='0 */12 * * *'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_12h}</button>
                        <button type="button" onclick="document.getElementById('cron-input').value='0 3 * * 0'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_weekly}</button>
                    </div>
                </div>
            </div>

            <!-- Bloco 2: Criptografia -->
            <div class="pt-4 border-t border-white/[0.06] space-y-3">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{encryption_section}</h4>
                </div>

                <div>
                    <label for="enc-pass" class="block text-xs font-mono text-slate-400 mb-1.5">{encryption_password_label}</label>
                    <div class="relative w-full sm:max-w-md">
                        <input type="password" id="enc-pass" name="encryption_password" value="" placeholder="{enc_pass_ph}" autocomplete="new-password" class="w-full px-3 py-2 pr-10 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                        <button type="button" onclick="const f=document.getElementById('enc-pass');f.type=f.type==='password'?'text':'password';" class="absolute right-2.5 top-2 text-slate-400 hover:text-white text-xs font-mono" title="Toggle visibility">👁️</button>
                    </div>
                    <p class="text-[11px] text-slate-500 font-sans mt-1">O arquivo .zip contendo banco, attachments, sends, chaves e templates será criptografado antes do envio.</p>
                </div>
            </div>

            <!-- Bloco 3: pCloud API & OAuth -->
            <div class="pt-4 border-t border-white/[0.06] space-y-4">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{pcloud_section}</h4>
                </div>

                {pcloud_status_card}

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                        <label for="pcloud-cid" class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_client_id_label}</label>
                        <input type="text" id="pcloud-cid" name="pcloud_client_id" value="{pcloud_client_id_val}" placeholder="{pcloud_client_id_ph}" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    </div>

                    <div>
                        <label for="pcloud-csec" class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_client_secret_label}</label>
                        <div class="relative w-full">
                            <input type="password" id="pcloud-csec" name="pcloud_client_secret" value="" placeholder="{pcloud_client_secret_ph}" autocomplete="off" class="w-full px-3 py-2 pr-10 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                            <button type="button" onclick="const f=document.getElementById('pcloud-csec');f.type=f.type==='password'?'text':'password';" class="absolute right-2.5 top-2 text-slate-400 hover:text-white text-xs font-mono" title="Toggle visibility">👁️</button>
                        </div>
                    </div>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                        <label class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_region_label}</label>
                        <select name="pcloud_region" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                            <option value="EU" {region_eu_selected}>{pcloud_region_eu}</option>
                            <option value="US" {region_us_selected}>{pcloud_region_us}</option>
                        </select>
                        <p class="text-[11px] text-slate-500 font-sans mt-1">Detectado automaticamente durante o login OAuth.</p>
                    </div>

                    <div>
                        <label class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_folder_label}</label>
                        <input type="text" name="pcloud_folder" value="{pcloud_folder_val}" placeholder="{pcloud_folder_ph}" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    </div>
                </div>

                <div class="p-3 rounded-lg bg-white/[0.02] border border-white/[0.06] text-xs font-mono">
                    <div class="flex items-center justify-between gap-2 mb-1">
                        <span class="text-slate-400">{pcloud_callback_url_label}:</span>
                        <button type="button" onclick="navigator.clipboard.writeText('{callback_url}');this.innerText='Copiado!';" class="px-2 py-0.5 rounded bg-white/[0.05] hover:bg-white/[0.1] text-slate-300 border border-white/[0.08] text-[11px] transition-colors">Copiar</button>
                    </div>
                    <div class="text-rose-300 font-semibold break-all select-all">{callback_url}</div>
                    <p class="text-[11px] text-slate-500 font-sans mt-1.5">{pcloud_callback_hint}</p>
                </div>
            </div>

            <!-- Bloco 4: Diagnóstico de Caminhos Locais -->
            <div class="pt-4 border-t border-white/[0.06] space-y-3">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{local_paths_section}</h4>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs font-mono">
                    <div class="p-3 rounded-lg bg-canvas border border-white/[0.06] flex items-center justify-between gap-2">
                        <span class="text-slate-400 truncate">{data_path_label}</span>
                        {data_path_badge}
                    </div>
                    <div class="p-3 rounded-lg bg-canvas border border-white/[0.06] flex items-center justify-between gap-2">
                        <span class="text-slate-400 truncate">{env_path_label}</span>
                        {env_path_badge}
                    </div>
                </div>
            </div>

            <div class="pt-4 flex justify-end">
                <button type="submit" class="px-5 py-2.5 rounded-lg text-xs font-semibold font-sans transition-all duration-150 flex items-center gap-2 bg-rose-600 hover:bg-rose-500 active:translate-y-px text-white border border-rose-400/30 shadow-md">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
                    <span>{btn_save_config}</span>
                </button>
            </div>
        </form>
    </div>

    <!-- CARD 3: HISTÓRICO DE BACKUPS -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="mb-4 pb-3 border-b border-white/[0.06] flex items-center justify-between">
            <div>
                <h3 class="text-lg font-display font-bold tracking-tight text-white">{history_section}</h3>
            </div>
            <button hx-get="/api/backups/fragment" hx-target="#backups-content" hx-swap="innerHTML" class="p-1.5 rounded-md hover:bg-white/[0.06] text-slate-400 hover:text-white transition-colors" title="Atualizar Histórico">
                <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
            </button>
        </div>

        <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="border-b border-white/[0.08] text-[11px] font-mono uppercase text-slate-400">
                        <th class="py-2.5 px-3">{table_date}</th>
                        <th class="py-2.5 px-3">{table_filename}</th>
                        <th class="py-2.5 px-3">{table_size}</th>
                        <th class="py-2.5 px-3">{table_status}</th>
                        <th class="py-2.5 px-3 text-right">{table_action}</th>
                    </tr>
                </thead>
                <tbody>
                    {history_rows}
                </tbody>
            </table>
        </div>
    </div>

    <script>
    function startPCloudOAuth() {{
        const cidEl = document.getElementById('pcloud-cid');
        const csecEl = document.getElementById('pcloud-csec');
        const form = document.createElement('form');
        form.method = 'POST';
        form.action = '/api/backups/pcloud/authorize';
        if (cidEl && cidEl.value.trim()) {{
            const input = document.createElement('input');
            input.type = 'hidden';
            input.name = 'client_id';
            input.value = cidEl.value.trim();
            form.appendChild(input);
        }}
        if (csecEl && csecEl.value.trim()) {{
            const input = document.createElement('input');
            input.type = 'hidden';
            input.name = 'client_secret';
            input.value = csecEl.value.trim();
            form.appendChild(input);
        }}
        document.body.appendChild(form);
        form.submit();
    }}
    </script>
</div>"##,
        alert_banner = alert_banner,
        backups_title = i18n.dashboard.backups_title,
        backups_subtitle = i18n.dashboard.backups_subtitle,
        status_badge = status_badge,
        next_run_label = i18n.backups.next_run,
        next_run_val = next_run_str,
        run_btn_disabled = run_btn_disabled,
        run_btn_text = run_btn_text,
        section_config = i18n.backups.section_config,
        enable_cron = i18n.backups.enable_cron,
        cron_checked = cron_checked,
        cron_expression_label = i18n.backups.cron_expression_label,
        cron_val = config.cron_expression,
        cron_hint = i18n.backups.cron_hint,
        preset_daily = i18n.backups.cron_preset_daily,
        preset_12h = i18n.backups.cron_preset_12h,
        preset_weekly = i18n.backups.cron_preset_weekly,
        encryption_section = i18n.backups.encryption_section,
        encryption_password_label = i18n.backups.encryption_password_label,
        enc_pass_ph = enc_pass_ph,
        pcloud_section = i18n.backups.pcloud_section,
        pcloud_status_card = pcloud_status_card,
        pcloud_client_id_label = i18n.backups.pcloud_client_id_label,
        pcloud_client_id_val = client_id_val,
        pcloud_client_id_ph = i18n.backups.pcloud_client_id_placeholder,
        pcloud_client_secret_label = i18n.backups.pcloud_client_secret_label,
        pcloud_client_secret_ph = client_secret_ph,
        pcloud_region_label = i18n.backups.pcloud_region_label,
        pcloud_region_eu = i18n.backups.pcloud_region_eu,
        pcloud_region_us = i18n.backups.pcloud_region_us,
        region_eu_selected = region_eu_selected,
        region_us_selected = region_us_selected,
        pcloud_folder_label = i18n.backups.pcloud_folder_label,
        pcloud_folder_val = config.pcloud_folder,
        pcloud_folder_ph = i18n.backups.pcloud_folder_placeholder,
        pcloud_callback_url_label = i18n.backups.pcloud_callback_url_label,
        pcloud_callback_hint = i18n.backups.pcloud_callback_hint,
        callback_url = callback_url,
        local_paths_section = i18n.backups.local_paths_section,
        data_path_label = i18n.backups.data_path_label,
        data_path_badge = data_path_badge,
        env_path_label = i18n.backups.env_path_label,
        env_path_badge = env_path_badge,
        btn_save_config = i18n.backups.btn_save_config,
        history_section = i18n.backups.history_section,
        table_date = i18n.backups.table_date,
        table_filename = i18n.backups.table_filename,
        table_size = i18n.backups.table_size,
        table_status = i18n.backups.table_status,
        table_action = i18n.backups.table_action,
        history_rows = history_rows,
    )
}

