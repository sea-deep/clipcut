use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A single clip defined by start and end in seconds.
/// Seconds representation crosses the IPC boundary; TypeScript parses display strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

impl Clip {
    pub fn duration(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    /// Validate this clip against the video's duration.
    pub fn validate(&self, video_duration: f64) -> Result<(), AppError> {
        if self.start_seconds < 0.0 {
            return Err(AppError::InvalidTimestamp(
                "Start time cannot be negative.".into(),
            ));
        }
        if !self.start_seconds.is_finite() || !self.end_seconds.is_finite() {
            return Err(AppError::InvalidTimestamp(
                "Timestamps must be valid numbers.".into(),
            ));
        }
        if self.end_seconds <= self.start_seconds {
            return Err(AppError::InvalidTimestamp(
                "End time must be after start time.".into(),
            ));
        }
        if self.end_seconds > video_duration + 1.0 {
            // Allow 1s tolerance for rounding
            return Err(AppError::InvalidTimestamp(format!(
                "End time {:.1}s exceeds video duration {:.1}s.",
                self.end_seconds, video_duration
            )));
        }
        Ok(())
    }
}

/// Metadata returned to the frontend after loading a video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration_seconds: f64,
    pub thumbnail_url: String,
}

/// Options for an export operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub output_directory: String,
    pub mode: ExportMode,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportMode {
    Accurate,
    Fast,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Mp4,
    Webm,
    Mkv,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Webm => "webm",
            OutputFormat::Mkv => "mkv",
        }
    }
}

/// Final result returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub files: Vec<String>,
    pub output_directory: String,
    pub cancelled: bool,
    pub clips_completed: usize,
    pub clips_total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_valid() {
        let c = Clip {
            start_seconds: 10.0,
            end_seconds: 30.0,
        };
        assert!(c.validate(120.0).is_ok());
    }

    #[test]
    fn clip_end_before_start() {
        let c = Clip {
            start_seconds: 30.0,
            end_seconds: 10.0,
        };
        assert!(c.validate(120.0).is_err());
    }

    #[test]
    fn clip_equal_times() {
        let c = Clip {
            start_seconds: 10.0,
            end_seconds: 10.0,
        };
        assert!(c.validate(120.0).is_err());
    }

    #[test]
    fn clip_beyond_duration() {
        let c = Clip {
            start_seconds: 100.0,
            end_seconds: 200.0,
        };
        assert!(c.validate(120.0).is_err());
    }

    #[test]
    fn clip_negative_start() {
        let c = Clip {
            start_seconds: -5.0,
            end_seconds: 10.0,
        };
        assert!(c.validate(120.0).is_err());
    }

    #[test]
    fn clip_duration() {
        let c = Clip {
            start_seconds: 15.0,
            end_seconds: 42.0,
        };
        assert!((c.duration() - 27.0).abs() < f64::EPSILON);
    }
}
