use cid::Cid;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenRouterError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Unresolved bond: {0}")]
    UnresolvedBond(Cid),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
