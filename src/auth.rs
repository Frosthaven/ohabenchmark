use base64::{engine::general_purpose::STANDARD, Engine};

use crate::cli::AuthType;
use crate::config::AuthConfig;

/// Generate the Authorization header value from auth config
pub fn generate_auth_header(config: &AuthConfig) -> Option<String> {
    match config.auth_type {
        AuthType::None => None,
        AuthType::Basic => {
            let username = config.username.as_deref().unwrap_or("");
            let password = config.password.as_deref().unwrap_or("");
            let credentials = format!("{}:{}", username, password);
            let encoded = STANDARD.encode(credentials.as_bytes());
            Some(format!("Authorization: Basic {}", encoded))
        }
        AuthType::Bearer => {
            let token = config.token.as_deref().unwrap_or("");
            Some(format!("Authorization: Bearer {}", token))
        }
        AuthType::Header => config.custom_header.clone(),
    }
}

/// Get auth type options for menu display
pub fn get_auth_type_names() -> Vec<&'static str> {
    vec!["None", "Basic Auth", "Bearer Token", "API Key Header"]
}

/// Convert menu index to AuthType
pub fn index_to_auth_type(index: usize) -> AuthType {
    match index {
        0 => AuthType::None,
        1 => AuthType::Basic,
        2 => AuthType::Bearer,
        3 => AuthType::Header,
        _ => AuthType::None,
    }
}
