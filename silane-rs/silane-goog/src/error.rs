use thiserror::Error;

#[derive(Debug, Error)]
pub enum GoogleError {
    #[error("API error: {0}")]
    Api(String),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("Parse error: {0}")]
    Parse(String),
}
