/// Configuration for the Kiro CLI client.
#[derive(Debug, Clone)]
pub struct KiroClientConfig {
    /// Path to the `kiro` CLI binary.
    pub binary_path: String,
    /// Default timeout for requests, in milliseconds.
    pub timeout_ms: Option<u64>,
}
