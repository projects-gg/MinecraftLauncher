pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Discord is not running (no IPC endpoint accepted the connection)")]
    NotRunning,

    #[error("Discord IPC I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid IPC frame: {0}")]
    Frame(String),

    #[error("invalid JSON on the Discord IPC connection: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Discord closed the connection ({code}): {message}")]
    Closed { code: i64, message: String },

    #[error("Discord returned error {code}: {message}")]
    Rpc { code: i64, message: String },

    #[error("timed out waiting for Discord")]
    Timeout,

    #[error("the Discord IPC connection was lost")]
    Disconnected,

    #[error("a tokio runtime is required")]
    NoRuntime,
}
