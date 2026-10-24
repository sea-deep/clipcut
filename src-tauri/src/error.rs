use thiserror::Error;

/// Application-level error categories.
/// Each variant maps to a user-facing message; internal details go to logs.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Please enter a valid YouTube URL.")]
    InvalidUrl,

    #[error("This video is unavailable or has been removed.")]
    VideoUnavailable,

    #[error("YouTube requires authentication for this video.")]
    AuthenticationRequired,

    #[error("Network error. Check your internet connection and try again.")]
    NetworkFailure,

    #[error("Not enough disk space to complete the export.")]
    DiskFull,

    #[error("ClipCut doesn't have permission to write to that folder.")]
    PermissionDenied,

    #[error("ClipCut's video tools encountered an error: {detail}")]
    ToolFailure { detail: String },

    #[error("{0}")]
    InvalidTimestamp(String),

    #[error("Operation cancelled.")]
    Cancelled,

    #[error("This media format is not supported.")]
    UnsupportedMedia,

    #[error("An unexpected error occurred: {0}")]
    Unknown(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", &self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidUrl => "INVALID_URL",
            AppError::VideoUnavailable => "VIDEO_UNAVAILABLE",
            AppError::AuthenticationRequired => "AUTH_REQUIRED",
            AppError::NetworkFailure => "NETWORK_FAILURE",
            AppError::DiskFull => "DISK_FULL",
            AppError::PermissionDenied => "PERMISSION_DENIED",
            AppError::ToolFailure { .. } => "TOOL_FAILURE",
            AppError::InvalidTimestamp(_) => "INVALID_TIMESTAMP",
            AppError::Cancelled => "CANCELLED",
            AppError::UnsupportedMedia => "UNSUPPORTED_MEDIA",
            AppError::Unknown(_) => "UNKNOWN",
        }
    }

    /// Classify a yt-dlp stderr string into the appropriate AppError.
    pub fn from_ytdlp_stderr(stderr: &str) -> Self {
        let lower = stderr.to_lowercase();
        if lower.contains("sign in") || lower.contains("confirm you") || lower.contains("bot") {
            AppError::AuthenticationRequired
        } else if lower.contains("video unavailable")
            || lower.contains("private video")
            || lower.contains("removed")
            || lower.contains("does not exist")
        {
            AppError::VideoUnavailable
        } else if lower.contains("urlopen error")
            || lower.contains("connection")
            || lower.contains("network")
            || lower.contains("timed out")
        {
            AppError::NetworkFailure
        } else if lower.contains("no space left") {
            AppError::DiskFull
        } else if lower.contains("permission denied") {
            AppError::PermissionDenied
        } else {
            let error_lines: Vec<&str> = stderr
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with("WARNING:"))
                .collect();
            let detail = if !error_lines.is_empty() {
                error_lines.join(" ")
            } else {
                stderr.trim().to_string()
            };
            AppError::ToolFailure { detail }
        }
    }

    /// Classify ffmpeg stderr into the appropriate AppError.
    pub fn from_ffmpeg_stderr(stderr: &str) -> Self {
        let lower = stderr.to_lowercase();
        if lower.contains("no space left") {
            AppError::DiskFull
        } else if lower.contains("permission denied") {
            AppError::PermissionDenied
        } else {
            let error_lines: Vec<&str> = stderr
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with("WARNING:"))
                .collect();
            let detail = if !error_lines.is_empty() {
                error_lines.join(" ")
            } else {
                stderr.trim().to_string()
            };
            AppError::ToolFailure { detail }
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => AppError::PermissionDenied,
            std::io::ErrorKind::StorageFull => AppError::DiskFull,
            _ => AppError::Unknown(e.to_string()),
        }
    }
}
