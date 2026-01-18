use polyepoxide_core::oxide;

/// Generation parameters used when producing a message.
#[oxide]
pub struct GenerationParams {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<u32>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub stop: Option<Vec<String>>,
    // Extended parameters for broader provider compatibility
    pub min_p: Option<f64>,
    pub top_a: Option<f64>,
    pub repetition_penalty: Option<f64>,
    pub seed: Option<u64>,
    // Reasoning/thinking support
    /// Reasoning effort level: "low", "medium", or "high"
    pub reasoning_effort: Option<String>,
    pub reasoning_max_tokens: Option<u32>,
}

/// Token usage statistics for a generation.
#[oxide]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
}

/// Metadata associated with a message.
#[oxide]
pub struct MessageMetadata {
    /// Model used to generate this message (if assistant-generated).
    pub model: Option<String>,
    /// Timestamp when the message was created (milliseconds since epoch).
    pub timestamp_ms: Option<u64>,
    /// Parameters used for generation.
    pub generation_params: Option<GenerationParams>,
    /// Reason the model stopped generating.
    pub stop_reason: Option<String>,
    /// Token usage statistics.
    pub usage: Option<TokenUsage>,
}
