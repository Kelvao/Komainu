use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Translations {
    pub lang: String,
    pub common: CommonTranslations,
    pub login: LoginTranslations,
    pub register: RegisterTranslations,
    pub dashboard: DashboardTranslations,
    pub metrics: MetricsTranslations,
    pub cli: CliTranslations,
    pub errors: ErrorsTranslations,
    #[serde(default)]
    pub backups: BackupsTranslations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonTranslations {
    pub app_title: String,
    pub app_subtitle: String,
    pub active_monitoring: String,
    pub logout: String,
    pub footer: String,
    pub operation_failed: String,
    pub session_expired: String,
    pub unauthorized: String,
    pub authenticated_success: String,
    pub passkey_registered_success: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginTranslations {
    pub page_title: String,
    pub header_title: String,
    pub header_subtitle: String,
    pub no_admin_warning: String,
    pub description: String,
    pub button: String,
    pub waiting_passkey: String,
    pub start_error: String,
    pub auth_error: String,
    pub default_error: String,
    #[serde(default = "default_tab_local")]
    pub tab_local: String,
    #[serde(default = "default_tab_qr")]
    pub tab_qr: String,
    #[serde(default = "default_qr_desc")]
    pub qr_desc: String,
    #[serde(default = "default_qr_waiting")]
    pub qr_waiting: String,
    #[serde(default = "default_qr_scanned")]
    pub qr_scanned: String,
    #[serde(default = "default_qr_approved")]
    pub qr_approved: String,
    #[serde(default = "default_qr_expired")]
    pub qr_expired: String,
    #[serde(default = "default_qr_reload")]
    pub qr_reload: String,
    #[serde(default = "default_mobile_title")]
    pub mobile_title: String,
    #[serde(default = "default_mobile_subtitle")]
    pub mobile_subtitle: String,
    #[serde(default = "default_mobile_btn")]
    pub mobile_btn: String,
    #[serde(default = "default_mobile_success_title")]
    pub mobile_success_title: String,
    #[serde(default = "default_mobile_success_desc")]
    pub mobile_success_desc: String,
}

fn default_tab_local() -> String { "Neste dispositivo".to_string() }
fn default_tab_qr() -> String { "Celular (QR Code)".to_string() }
fn default_qr_desc() -> String { "Aponte a câmera do seu smartphone para o QR Code para usar o celular como chave de acesso.".to_string() }
fn default_qr_waiting() -> String { "Aguardando leitura do QR Code pelo celular...".to_string() }
fn default_qr_scanned() -> String { "Celular conectado! Confirme a biometria no smartphone...".to_string() }
fn default_qr_approved() -> String { "Acesso autorizado! Redirecionando...".to_string() }
fn default_qr_expired() -> String { "QR Code expirado. Clique para recarregar.".to_string() }
fn default_qr_reload() -> String { "Gerar novo QR Code".to_string() }
fn default_mobile_title() -> String { "Autorizar Login no Computador".to_string() }
fn default_mobile_subtitle() -> String { "Use sua biometria ou passkey para desbloquear a sessão no PC.".to_string() }
fn default_mobile_btn() -> String { "Liberar Acesso no PC".to_string() }
fn default_mobile_success_title() -> String { "Acesso Liberado com Sucesso!".to_string() }
fn default_mobile_success_desc() -> String { "O login no computador foi confirmado. Você já pode fechar esta aba.".to_string() }


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterTranslations {
    pub page_title: String,
    pub header_title: String,
    pub header_subtitle: String,
    pub description: String,
    pub button: String,
    pub creating_credential: String,
    pub start_error: String,
    pub save_error: String,
    pub default_error: String,
    pub success_alert: String,
    pub token_missing: String,
    pub token_invalid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTranslations {
    pub page_title: String,
    pub instance_title: String,
    pub image_prefix: String,
    pub started_prefix: String,
    pub status_running: String,
    pub status_stopped: String,
    pub status_restarting: String,
    pub status_unknown: String,
    pub btn_start: String,
    pub btn_stop: String,
    pub btn_restart: String,
    pub btn_update: String,
    pub btn_rebuild: String,
    pub confirm_rebuild: String,
    #[serde(default = "default_nav_home")]
    pub nav_home: String,
    #[serde(default = "default_nav_backups")]
    pub nav_backups: String,
    #[serde(default = "default_backups_title")]
    pub backups_title: String,
    #[serde(default = "default_backups_subtitle")]
    pub backups_subtitle: String,
    #[serde(default = "default_backups_placeholder_title")]
    pub backups_placeholder_title: String,
    #[serde(default = "default_backups_placeholder_desc")]
    pub backups_placeholder_desc: String,
}

fn default_nav_home() -> String { "Home".to_string() }
fn default_nav_backups() -> String { "Backups".to_string() }
fn default_backups_title() -> String { "Cópias de Segurança (Backups)".to_string() }
fn default_backups_subtitle() -> String { "Gestão e rotinas de salvaguarda do Vaultwarden".to_string() }
fn default_backups_placeholder_title() -> String { "Módulo de Backups em Preparação".to_string() }
fn default_backups_placeholder_desc() -> String { "Em breve você poderá gerar snapshots instantâneos, agendar rotinas automáticas de retenção e restaurar dados diretamente por este painel.".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsTranslations {
    pub cpu_title: String,
    pub ram_title: String,
    pub disk_title: String,
    pub container_label: String,
    pub host_label: String,
    pub occupied_suffix: String,
    #[serde(default = "default_container_section")]
    pub container_section: String,
    #[serde(default = "default_host_section")]
    pub host_section: String,
    #[serde(default = "default_container_cpu")]
    pub container_cpu: String,
    #[serde(default = "default_container_ram")]
    pub container_ram: String,
}

fn default_container_section() -> String { "Consumo do Container".to_string() }
fn default_host_section() -> String { "Consumo Geral da Máquina (Host)".to_string() }
fn default_container_cpu() -> String { "CPU do Container".to_string() }
fn default_container_ram() -> String { "RAM do Container".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliTranslations {
    pub reg_blocked_title: String,
    pub reg_blocked_admin_exists: String,
    pub reg_blocked_single_admin: String,
    pub reg_blocked_force_hint: String,
    pub reg_title: String,
    pub reg_scan_prompt: String,
    pub reg_direct_link: String,
    pub reg_single_use: String,
    pub reg_ctrl_c: String,
    pub warn_no_admin_title: String,
    pub warn_no_admin_help: String,
    pub server_starting: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorsTranslations {
    pub container_not_found: String,
    pub docker_socket_unavailable: String,
    pub docker_generic: String,
    pub container_start_failed: String,
    pub container_stop_failed: String,
    pub container_restart_failed: String,
    pub image_pull_failed: String,
    pub container_rebuild_failed: String,
    pub no_admin_registered: String,
    pub already_registered: String,
    pub invalid_token: String,
    pub session_expired: String,
    pub store_error: String,
    pub webauthn_error: String,
    pub unauthorized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupsTranslations {
    #[serde(default = "default_b_status")]
    pub section_status: String,
    #[serde(default = "default_b_now")]
    pub btn_backup_now: String,
    #[serde(default = "default_b_running")]
    pub btn_running: String,
    #[serde(default = "default_b_cron_en")]
    pub cron_enabled: String,
    #[serde(default = "default_b_cron_dis")]
    pub cron_disabled: String,
    #[serde(default = "default_b_next_run")]
    pub next_run: String,
    #[serde(default = "default_b_no_sched")]
    pub no_scheduled_run: String,
    #[serde(default = "default_b_sec_cfg")]
    pub section_config: String,
    #[serde(default = "default_b_en_cron")]
    pub enable_cron: String,
    #[serde(default = "default_b_cron_expr")]
    pub cron_expression_label: String,
    #[serde(default = "default_b_cron_hint")]
    pub cron_hint: String,
    #[serde(default = "default_b_preset_daily")]
    pub cron_preset_daily: String,
    #[serde(default = "default_b_preset_12h")]
    pub cron_preset_12h: String,
    #[serde(default = "default_b_preset_weekly")]
    pub cron_preset_weekly: String,
    #[serde(default = "default_b_sec_enc")]
    pub encryption_section: String,
    #[serde(default = "default_b_enc_pass")]
    pub encryption_password_label: String,
    #[serde(default = "default_b_enc_pass_ph")]
    pub encryption_password_placeholder: String,
    #[serde(default = "default_b_pcloud_sec")]
    pub pcloud_section: String,
    #[serde(default = "default_b_pcloud_reg")]
    pub pcloud_region_label: String,
    #[serde(default = "default_b_pcloud_eu")]
    pub pcloud_region_eu: String,
    #[serde(default = "default_b_pcloud_us")]
    pub pcloud_region_us: String,
    #[serde(default = "default_b_pcloud_tok")]
    pub pcloud_token_label: String,
    #[serde(default = "default_b_pcloud_tok_ph")]
    pub pcloud_token_placeholder: String,
    #[serde(default = "default_b_pcloud_fld")]
    pub pcloud_folder_label: String,
    #[serde(default = "default_b_pcloud_fld_ph")]
    pub pcloud_folder_placeholder: String,
    #[serde(default = "default_b_pcloud_oauth_btn")]
    pub pcloud_oauth_btn: String,
    #[serde(default = "default_b_pcloud_reconnect_btn")]
    pub pcloud_reconnect_btn: String,
    #[serde(default = "default_b_pcloud_disconnect_btn")]
    pub pcloud_disconnect_btn: String,
    #[serde(default = "default_b_pcloud_conn")]
    pub pcloud_connected: String,
    #[serde(default = "default_b_pcloud_not_conn")]
    pub pcloud_not_connected: String,
    #[serde(default = "default_b_pcloud_cid_lbl")]
    pub pcloud_client_id_label: String,
    #[serde(default = "default_b_pcloud_cid_ph")]
    pub pcloud_client_id_placeholder: String,
    #[serde(default = "default_b_pcloud_csec_lbl")]
    pub pcloud_client_secret_label: String,
    #[serde(default = "default_b_pcloud_csec_ph")]
    pub pcloud_client_secret_placeholder: String,
    #[serde(default = "default_b_pcloud_cb_url")]
    pub pcloud_callback_url_label: String,
    #[serde(default = "default_b_pcloud_cb_hint")]
    pub pcloud_callback_hint: String,
    #[serde(default = "default_b_pcloud_auth_succ")]
    pub pcloud_auth_success: String,
    #[serde(default = "default_b_pcloud_auth_fail")]
    pub pcloud_auth_failed: String,
    #[serde(default = "default_b_pcloud_disc_succ")]
    pub pcloud_disconnected_success: String,
    #[serde(default = "default_b_loc_sec")]
    pub local_paths_section: String,
    #[serde(default = "default_b_data_lbl")]
    pub data_path_label: String,
    #[serde(default = "default_b_env_lbl")]
    pub env_path_label: String,
    #[serde(default = "default_b_found")]
    pub path_found: String,
    #[serde(default = "default_b_missing")]
    pub path_missing: String,
    #[serde(default = "default_b_save")]
    pub btn_save_config: String,
    #[serde(default = "default_b_hist_sec")]
    pub history_section: String,
    #[serde(default = "default_b_hist_empty")]
    pub history_empty: String,
    #[serde(default = "default_b_tbl_date")]
    pub table_date: String,
    #[serde(default = "default_b_tbl_fn")]
    pub table_filename: String,
    #[serde(default = "default_b_tbl_sz")]
    pub table_size: String,
    #[serde(default = "default_b_tbl_dur")]
    pub table_duration: String,
    #[serde(default = "default_b_tbl_st")]
    pub table_status: String,
    #[serde(default = "default_b_tbl_act")]
    pub table_action: String,
    #[serde(default = "default_b_btn_dl_dec")]
    pub btn_download_decrypted: String,
    #[serde(default = "default_b_st_remote")]
    pub status_remote: String,
    #[serde(default = "default_b_st_succ")]
    pub status_success: String,
    #[serde(default = "default_b_st_fail")]
    pub status_failed: String,
    #[serde(default = "default_b_cfg_saved")]
    pub config_saved_success: String,
    #[serde(default = "default_b_bkp_succ")]
    pub backup_started_success: String,
    #[serde(default = "default_b_already_run")]
    pub backup_already_running: String,
}

fn default_b_status() -> String { "Status do Agendamento & Operação".to_string() }
fn default_b_now() -> String { "Fazer Backup Agora".to_string() }
fn default_b_running() -> String { "Executando Backup...".to_string() }
fn default_b_cron_en() -> String { "Agendamento Automático Ativo".to_string() }
fn default_b_cron_dis() -> String { "Agendamento Desativado".to_string() }
fn default_b_next_run() -> String { "Próxima execução estimada:".to_string() }
fn default_b_no_sched() -> String { "Nenhuma rotina agendada".to_string() }
fn default_b_sec_cfg() -> String { "Configuração da Rotina de Backup".to_string() }
fn default_b_en_cron() -> String { "Habilitar agendamento automático via Cron".to_string() }
fn default_b_cron_expr() -> String { "Expressão Cron".to_string() }
fn default_b_cron_hint() -> String { "Formato: minuto hora dia mês dia-da-semana (ex: 0 3 * * * = diariamente às 03:00)".to_string() }
fn default_b_preset_daily() -> String { "Diário (03:00)".to_string() }
fn default_b_preset_12h() -> String { "A cada 12 horas".to_string() }
fn default_b_preset_weekly() -> String { "Semanal (Domingo 03:00)".to_string() }
fn default_b_sec_enc() -> String { "Criptografia de Dados (AES-256-GCM)".to_string() }
fn default_b_enc_pass() -> String { "Senha de Criptografia do Pacote".to_string() }
fn default_b_enc_pass_ph() -> String { "Digite uma senha forte para criptografar o backup".to_string() }
fn default_b_pcloud_sec() -> String { "Destino em Nuvem: pCloud API".to_string() }
fn default_b_pcloud_reg() -> String { "Região do Data Center pCloud".to_string() }
fn default_b_pcloud_eu() -> String { "Europa (eapi.pcloud.com)".to_string() }
fn default_b_pcloud_us() -> String { "Estados Unidos (api.pcloud.com)".to_string() }
fn default_b_pcloud_tok() -> String { "Access Token OAuth do pCloud".to_string() }
fn default_b_pcloud_tok_ph() -> String { "Cole seu token de acesso da API do pCloud".to_string() }
fn default_b_pcloud_fld() -> String { "Pasta de Destino no pCloud (ID ou Caminho)".to_string() }
fn default_b_pcloud_fld_ph() -> String { "0 (raiz) ou /Backups/Vaultwarden".to_string() }
fn default_b_pcloud_oauth_btn() -> String { "Conectar com pCloud (OAuth)".to_string() }
fn default_b_pcloud_reconnect_btn() -> String { "Reconectar com pCloud".to_string() }
fn default_b_pcloud_disconnect_btn() -> String { "Desconectar".to_string() }
fn default_b_pcloud_conn() -> String { "Conectado ao pCloud".to_string() }
fn default_b_pcloud_not_conn() -> String { "Não conectado ao pCloud".to_string() }
fn default_b_pcloud_cid_lbl() -> String { "Client ID da Aplicação (App Key)".to_string() }
fn default_b_pcloud_cid_ph() -> String { "Insira o Client ID do seu app no pCloud".to_string() }
fn default_b_pcloud_csec_lbl() -> String { "Client Secret da Aplicação (App Secret)".to_string() }
fn default_b_pcloud_csec_ph() -> String { "Insira o Client Secret do seu app".to_string() }
fn default_b_pcloud_cb_url() -> String { "URL de Redirecionamento (Callback URI)".to_string() }
fn default_b_pcloud_cb_hint() -> String { "Cadastre exatamente esta URL no painel de desenvolvedor do seu app no pCloud (Redirect URI)".to_string() }
fn default_b_pcloud_auth_succ() -> String { "Autenticação com o pCloud realizada com sucesso!".to_string() }
fn default_b_pcloud_auth_fail() -> String { "Falha na autenticação OAuth com o pCloud".to_string() }
fn default_b_pcloud_disc_succ() -> String { "Conta do pCloud desconectada com sucesso.".to_string() }
fn default_b_loc_sec() -> String { "Diagnóstico de Arquivos e Pastas Locais (.env)".to_string() }
fn default_b_data_lbl() -> String { "Diretório de dados (/data):".to_string() }
fn default_b_env_lbl() -> String { "Arquivo de ambiente (vaultwarden.env):".to_string() }
fn default_b_found() -> String { "Acessível".to_string() }
fn default_b_missing() -> String { "Não encontrado no host".to_string() }
fn default_b_save() -> String { "Salvar Configurações".to_string() }
fn default_b_hist_sec() -> String { "Histórico de Backups".to_string() }
fn default_b_hist_empty() -> String { "Nenhum backup realizado até o momento.".to_string() }
fn default_b_tbl_date() -> String { "Data / Hora".to_string() }
fn default_b_tbl_fn() -> String { "Arquivo Gerado".to_string() }
fn default_b_tbl_sz() -> String { "Tamanho".to_string() }
fn default_b_tbl_dur() -> String { "Duração".to_string() }
fn default_b_tbl_st() -> String { "Status".to_string() }
fn default_b_tbl_act() -> String { "Ação".to_string() }
fn default_b_btn_dl_dec() -> String { "Baixar Descriptografado".to_string() }
fn default_b_st_remote() -> String { "Na Nuvem".to_string() }
fn default_b_st_succ() -> String { "Sucesso".to_string() }
fn default_b_st_fail() -> String { "Falha".to_string() }
fn default_b_cfg_saved() -> String { "Configurações de backup salvas com sucesso!".to_string() }
fn default_b_bkp_succ() -> String { "Backup executado e enviado com sucesso ao pCloud!".to_string() }
fn default_b_already_run() -> String { "Um backup já está em andamento. Aguarde a finalização.".to_string() }

static CACHED_TRANSLATIONS: std::sync::OnceLock<(std::sync::Arc<Translations>, std::sync::Arc<Translations>)> =
    std::sync::OnceLock::new();

pub struct I18n;

impl I18n {
    const EN_JSON: &'static str = include_str!("../locales/en.json");
    const PT_BR_JSON: &'static str = include_str!("../locales/pt-BR.json");

    pub fn get(lang: &str) -> std::sync::Arc<Translations> {
        let (en, pt) = CACHED_TRANSLATIONS.get_or_init(|| {
            (
                std::sync::Arc::new(Self::load("en")),
                std::sync::Arc::new(Self::load("pt-BR")),
            )
        });
        let normalized = lang.trim().to_lowercase();
        match normalized.as_str() {
            "pt" | "pt-br" | "pt_br" | "pt-pt" => pt.clone(),
            _ => en.clone(),
        }
    }

    pub fn load(lang: &str) -> Translations {
        let normalized = lang.trim().to_lowercase();
        let (file_name, embedded_json) = match normalized.as_str() {
            "pt" | "pt-br" | "pt_br" | "pt-pt" => ("pt-BR.json", Self::PT_BR_JSON),
            _ => ("en.json", Self::EN_JSON),
        };

        // If an external override file exists in locales/<file_name>, load it
        let external_path = format!("locales/{}", file_name);
        if let Ok(content) = std::fs::read_to_string(&external_path) {
            match serde_json::from_str::<Translations>(&content) {
                Ok(trans) => return trans,
                Err(err) => {
                    tracing::warn!(
                        "Falha ao ler arquivo de tradução externo '{}': {}. Usando padrão embutido.",
                        external_path,
                        err
                    );
                }
            }
        }

        serde_json::from_str(embedded_json).expect("Embedded translation JSON must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_english() {
        let t = I18n::load("en");
        assert_eq!(t.lang, "en");
        assert_eq!(t.common.app_title, "Komainu");
        assert_eq!(t.login.button, "Sign in with Passkey");
        assert_eq!(t.dashboard.btn_start, "Start");
    }

    #[test]
    fn test_load_portuguese() {
        let t = I18n::load("pt-BR");
        assert_eq!(t.lang, "pt-BR");
        assert_eq!(t.login.button, "Entrar com Passkey");
        assert_eq!(t.dashboard.btn_start, "Iniciar");
    }

    #[test]
    fn test_load_fallback_to_english() {
        let t = I18n::load("fr");
        assert_eq!(t.lang, "en");
    }

    #[test]
    fn test_get_cached_translations() {
        let en1 = I18n::get("en");
        let en2 = I18n::get("en");
        assert!(std::sync::Arc::ptr_eq(&en1, &en2));

        let pt1 = I18n::get("pt-BR");
        let pt2 = I18n::get("pt");
        assert!(std::sync::Arc::ptr_eq(&pt1, &pt2));
        assert_eq!(pt1.lang, "pt-BR");
    }
}

