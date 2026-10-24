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
    #[serde(default)]
    pub quality: QualityOption,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum QualityOption {
    #[default]
    Highest,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "360p")]
    P360,
}

impl QualityOption {
    pub fn format_selector(&self) -> &'static str {
        match self {
            QualityOption::Highest => "bv*[protocol^=http]+ba[protocol^=http]/b[protocol^=http]/best",
            QualityOption::P1080 => "bv*[height<=1080][protocol^=http]+ba[protocol^=http]/b[height<=1080][protocol^=http]/best",
            QualityOption::P720 => "bv*[height<=720][protocol^=http]+ba[protocol^=http]/b[height<=720][protocol^=http]/best",
            QualityOption::P480 => "bv*[height<=480][protocol^=http]+ba[protocol^=http]/b[height<=480][protocol^=http]/best",
            QualityOption::P360 => "bv*[height<=360][protocol^=http]+ba[protocol^=http]/b[height<=360][protocol^=http]/best",
        }
    }

    pub fn scale_filter(&self) -> Option<&'static str> {
        match self {
            QualityOption::Highest => None,
            QualityOption::P1080 => Some("scale=-2:'min(1080,ih)'"),
            QualityOption::P720 => Some("scale=-2:'min(720,ih)'"),
            QualityOption::P480 => Some("scale=-2:'min(480,ih)'"),
            QualityOption::P360 => Some("scale=-2:'min(360,ih)'"),
        }
    }
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

    #[test]
    fn quality_option_defaults_and_selectors() {
        assert_eq!(QualityOption::default(), QualityOption::Highest);
        assert!(QualityOption::Highest
            .format_selector()
            .contains("bv*[protocol^=http]"));
        assert!(QualityOption::P1080
            .format_selector()
            .contains("height<=1080"));
        assert!(QualityOption::P720
            .format_selector()
            .contains("height<=720"));
        assert!(QualityOption::P480
            .format_selector()
            .contains("height<=480"));
        assert!(QualityOption::P360
            .format_selector()
            .contains("height<=360"));

        assert_eq!(QualityOption::Highest.scale_filter(), None);
        assert_eq!(
            QualityOption::P720.scale_filter(),
            Some("scale=-2:'min(720,ih)'")
        );
        assert_eq!(
            QualityOption::P1080.scale_filter(),
            Some("scale=-2:'min(1080,ih)'")
        );

        let json = r#"{"output_directory":"/tmp","mode":"accurate","format":"mp4"}"#;
        let opt: ExportOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opt.quality, QualityOption::Highest);

        let json_720 =
            r#"{"output_directory":"/tmp","mode":"accurate","format":"mp4","quality":"720p"}"#;
        let opt_720: ExportOptions = serde_json::from_str(json_720).unwrap();
        assert_eq!(opt_720.quality, QualityOption::P720);
    }
}
