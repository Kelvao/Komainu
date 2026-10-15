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
    pub base_path: String,
    pub vw_data_path: String,
    pub backup_config_path: String,
}

pub(crate) fn clean_env_value(val: &str) -> String {
    if let Some((clean, _)) = val.split_once('#') {
        clean.trim().to_string()
    } else {
        val.trim().to_string()
    }
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

        let session_secret = env::var("SESSION_SECRET")
            .unwrap_or_else(|_| "dev_insecure_secret_key_change_in_prod_1234567890".to_string());

        if session_secret == "dev_insecure_secret_key_change_in_prod_1234567890" {
            tracing::warn!(
                "⚠️  AVISO DE SEGURANÇA: Usando SESSION_SECRET padrão inseguro! Configure a variável de ambiente SESSION_SECRET com uma chave aleatória de pelo menos 32 bytes para produção."
            );
        } else if session_secret.len() < 32 {
            tracing::warn!(
                "⚠️  AVISO DE SEGURANÇA: SESSION_SECRET possui menos de 32 caracteres. Recomenda-se gerar uma chave aleatória mais longa."
            );
        }

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

        let base_path_raw = env::var("BASE_PATH").unwrap_or_default();
        let base_path = {
            let without_comment = clean_env_value(&base_path_raw);
            let trimmed = without_comment.trim_matches('/');
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("/{}", trimmed)
            }
        };

        let vw_data_path_raw = env::var("VW_DATA_PATH").unwrap_or_else(|_| "vw-data".to_string());
        let vw_data_path = clean_env_value(&vw_data_path_raw);

        let backup_config_path_raw = env::var("BACKUP_CONFIG_PATH").unwrap_or_else(|_| "backup_config.json".to_string());
        let backup_config_path = clean_env_value(&backup_config_path_raw);

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
            base_path,
            vw_data_path,
            backup_config_path,
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
        assert_eq!(c.base_path, "");
    }

    #[test]
    fn test_clean_env_value_inline_comments() {
        assert_eq!(clean_env_value("/komainu # Optional: subpath"), "/komainu");
        assert_eq!(clean_env_value("pt-BR # Options: en, pt-BR"), "pt-BR");
        assert_eq!(clean_env_value("  127.0.0.1  "), "127.0.0.1");
        assert_eq!(clean_env_value("3000 # default port"), "3000");
    }
}
