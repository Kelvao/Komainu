use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub server_port: u16,
    pub docker_socket: String,
    pub session_secret: String,
    pub vw_container_name: String,
    pub rp_id: String,
    pub rp_origin: String,
    pub credentials_path: String,
    pub app_lang: String,
    pub base_path: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid SERVER_PORT: {e}"))?;

        let docker_socket = env::var("DOCKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/docker.sock".to_string());

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

        let vw_container_name = env::var("VW_CONTAINER_NAME")
            .unwrap_or_else(|_| "vaultwarden".to_string());

        let rp_id = env::var("RP_ID").unwrap_or_else(|_| "localhost".to_string());

        let rp_origin_raw = env::var("RP_ORIGIN")
            .unwrap_or_else(|_| format!("http://localhost:{server_port}"));
        let rp_origin = rp_origin_raw.trim().trim_end_matches('/').to_string();

        let credentials_path = env::var("CREDENTIALS_PATH")
            .unwrap_or_else(|_| "credentials.json".to_string());

        let app_lang = env::var("APP_LANG").unwrap_or_else(|_| "pt-BR".to_string());

        let base_path_raw = env::var("BASE_PATH").unwrap_or_default();
        let base_path = {
            let trimmed = base_path_raw.trim().trim_matches('/');
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("/{}", trimmed)
            }
        };

        Ok(Self {
            server_port,
            docker_socket,
            session_secret,
            vw_container_name,
            rp_id,
            rp_origin,
            credentials_path,
            app_lang,
            base_path,
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
        assert_eq!(c.server_port, 3000);
        assert_eq!(c.rp_id, "localhost");
        assert_eq!(c.app_lang, "pt-BR");
        assert_eq!(c.base_path, "");
    }
}
