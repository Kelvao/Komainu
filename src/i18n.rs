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
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsTranslations {
    pub cpu_title: String,
    pub ram_title: String,
    pub disk_title: String,
    pub container_label: String,
    pub host_label: String,
    pub occupied_suffix: String,
}

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

pub struct I18n;

impl I18n {
    const EN_JSON: &'static str = include_str!("../locales/en.json");
    const PT_BR_JSON: &'static str = include_str!("../locales/pt-BR.json");

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
}

