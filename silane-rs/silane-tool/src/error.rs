use thiserror::Error;

use crate::store::AnyStoreError;

#[derive(Debug, Error)]
pub enum SihError {
    #[error("API key not found. Set OPENROUTER_API_KEY or configure ~/.config/silane/config.toml")]
    ApiKeyNotFound,

    #[error("Google access token not found. Set GOOGLE_ACCESS_TOKEN or configure ~/.config/silane/config.toml")]
    GoogleTokenNotFound,

    #[error("Config error: {0}")]
    Config(#[from] toml::de::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Store error: {0}")]
    Store(#[from] AnyStoreError),

    #[error("Invalid CID: {0}")]
    InvalidCid(#[from] cid::Error),

    #[error("Message not found: {0}")]
    MessageNotFound(cid::Cid),

    #[error("Failed to decode message: {0}")]
    DecodeError(String),

    #[error("OpenRouter error: {0}")]
    OpenRouter(#[from] silane_openrouter::OpenRouterError),

    #[error("Google API error: {0}")]
    Google(#[from] silane_goog::GoogleError),
}
