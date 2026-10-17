use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub docker_socket: String,
    pub session_secret: String,
    pub vw_container_name: String,
    pub rp_id: String,
    pub rp_origin: String,
    pub credentials_path: String,
    pub app_lang: String,
    pub vw_data_path: String,
    pub backup_config_path: String,
    pub pcloud_client_id: String,
    pub pcloud_client_secret: String,
}

pub(crate) fn clean_env_value(val: &str) -> String {
    let trimmed = val.trim();
    if trimmed.starts_with('#') {
        return String::new();
    }
    let space_idx = trimmed.find(" #");
    let tab_idx = trimmed.find("\t#");
    let comment_idx = match (space_idx, tab_idx) {
        (Some(s), Some(t)) => Some(s.min(t)),
        (Some(s), None) => Some(s),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    };

    if let Some(pos) = comment_idx {
        trimmed[..pos].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn resolve_or_generate_session_secret() -> String {
    let secret_file = std::path::Path::new(".session_secret");
    if secret_file.exists() {
        if let Ok(content) = std::fs::read_to_string(secret_file) {
            let trimmed = content.trim();
            if trimmed.len() >= 32 {
                return trimmed.to_string();
            }
        }
    }

    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    if rng.fill(&mut bytes).is_ok() {
        let hex_secret = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let _ = (|| -> Result<(), std::io::Error> {
            let tmp_path = std::path::Path::new(".session_secret.tmp");
            std::fs::write(tmp_path, &hex_secret)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(tmp_path, std::fs::Permissions::from_mode(0o600));
            }
            std::fs::rename(tmp_path, secret_file)?;
            Ok(())
        })();
        tracing::info!("SESSION_SECRET was not provided: generated secure unique secret in .session_secret");
        return hex_secret;
    }

    "dev_insecure_secret_key_change_in_prod_1234567890".to_string()
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let server_host_raw = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let server_host = clean_env_value(&server_host_raw);

        let server_port_raw = env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
        let server_port = clean_env_value(&server_port_raw)
            .parse::<u16>()
            .map_err(|e| format!("Invalid SERVER_PORT: {e}"))?;

        let docker_socket_raw = env::var("DOCKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/docker.sock".to_string());
        let docker_socket = clean_env_value(&docker_socket_raw);

        let session_secret = match env::var("SESSION_SECRET") {
            Ok(s) => {
                let cleaned = clean_env_value(&s);
                if cleaned.is_empty() {
                    resolve_or_generate_session_secret()
                } else {
                    if cleaned == "dev_insecure_secret_key_change_in_prod_1234567890" {
                        tracing::warn!(
                            "⚠️  AVISO DE SEGURANÇA: Usando SESSION_SECRET padrão inseguro! Recomenda-se configurar uma chave aleatória no .env."
                        );
                    } else if cleaned.len() < 32 {
                        tracing::warn!(
                            "⚠️  AVISO DE SEGURANÇA: SESSION_SECRET possui menos de 32 caracteres. Recomenda-se gerar uma chave aleatória mais longa."
                        );
                    }
                    cleaned
                }
            }
            Err(_) => resolve_or_generate_session_secret(),
        };

        let vw_container_name_raw = env::var("VW_CONTAINER_NAME")
            .unwrap_or_else(|_| "vaultwarden".to_string());
        let vw_container_name = clean_env_value(&vw_container_name_raw);

        let rp_id_raw = env::var("RP_ID").unwrap_or_else(|_| "localhost".to_string());
        let rp_id = clean_env_value(&rp_id_raw);

        let rp_origin_raw = env::var("RP_ORIGIN")
            .unwrap_or_else(|_| format!("http://localhost:{server_port}"));
        let rp_origin = clean_env_value(&rp_origin_raw).trim_end_matches('/').to_string();

        let credentials_path_raw = env::var("CREDENTIALS_PATH")
            .unwrap_or_else(|_| "credentials.json".to_string());
        let credentials_path = clean_env_value(&credentials_path_raw);

        let app_lang_raw = env::var("APP_LANG").unwrap_or_else(|_| "pt-BR".to_string());
        let app_lang = clean_env_value(&app_lang_raw);

        let vw_data_path_raw = env::var("VW_DATA_PATH").unwrap_or_else(|_| "vw-data".to_string());
        let vw_data_path = clean_env_value(&vw_data_path_raw);

        let backup_config_path_raw = env::var("BACKUP_CONFIG_PATH").unwrap_or_else(|_| "backup_config.json".to_string());
        let backup_config_path = clean_env_value(&backup_config_path_raw);

        let pcloud_client_id_raw = env::var("PCLOUD_CLIENT_ID").unwrap_or_default();
        let pcloud_client_id = clean_env_value(&pcloud_client_id_raw);

        let pcloud_client_secret_raw = env::var("PCLOUD_CLIENT_SECRET").unwrap_or_default();
        let pcloud_client_secret = clean_env_value(&pcloud_client_secret_raw);

        Ok(Self {
            server_host,
            server_port,
            docker_socket,
            session_secret,
            vw_container_name,
            rp_id,
            rp_origin,
            credentials_path,
            app_lang,
            vw_data_path,
            backup_config_path,
            pcloud_client_id,
            pcloud_client_secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_default() {
        let config = Config::from_env();
        assert!(config.is_ok());
        let c = config.unwrap();
        assert_eq!(c.server_host, "127.0.0.1");
        assert_eq!(c.server_port, 3000);
        assert_eq!(c.rp_id, "localhost");
        assert_eq!(c.app_lang, "pt-BR");
    }

    #[test]
    fn test_clean_env_value_inline_comments() {
        assert_eq!(clean_env_value("/komainu # Optional: subpath"), "/komainu");
        assert_eq!(clean_env_value("pt-BR # Options: en, pt-BR"), "pt-BR");
        assert_eq!(clean_env_value("  127.0.0.1  "), "127.0.0.1");
        assert_eq!(clean_env_value("3000 # default port"), "3000");
        assert_eq!(clean_env_value("3000\t# tab comment"), "3000");
        assert_eq!(clean_env_value("s3cr3t#123!#abc"), "s3cr3t#123!#abc");
        assert_eq!(clean_env_value("s3cr3t#123! # comment"), "s3cr3t#123!");
        assert_eq!(clean_env_value("# full comment line"), "");
    }

    #[test]
    fn test_resolve_or_generate_session_secret() {
        let sec = resolve_or_generate_session_secret();
        assert!(sec.len() >= 32);
        assert_ne!(sec, "dev_insecure_secret_key_change_in_prod_1234567890");
        // Verify subsequent call loads the same persisted secret
        let sec2 = resolve_or_generate_session_secret();
        assert_eq!(sec, sec2);
        let _ = std::fs::remove_file(".session_secret");
    }
}

