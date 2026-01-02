//! Error types for VBAP operations.

use std::fmt;

/// Errors that can occur during VBAP configuration and computation.
#[derive(Debug, Clone, PartialEq)]
pub enum VBAPError {
    /// Not enough speakers for VBAP (minimum 2 for 2D, 3 for 3D).
    InsufficientSpeakers {
        /// Number of speakers provided.
        provided: usize,
        /// Minimum required for the requested dimension.
        required: usize,
    },

    /// Cannot form valid speaker pairs (2D) or triplets (3D).
    /// This can happen if speakers are too close together or all collinear.
    InvalidConfiguration(String),

    /// Azimuth or elevation angle is out of valid range.
    InvalidAngle {
        /// Name of the angle parameter.
        parameter: &'static str,
        /// The invalid value provided.
        value: f64,
        /// Minimum valid value.
        min: f64,
        /// Maximum valid value.
        max: f64,
    },
}

impl fmt::Display for VBAPError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VBAPError::InsufficientSpeakers { provided, required } => {
                write!(
                    f,
                    "insufficient speakers: {} provided, {} required",
                    provided, required
                )
            }
            VBAPError::InvalidConfiguration(msg) => {
                write!(f, "invalid speaker configuration: {}", msg)
            }
            VBAPError::InvalidAngle {
                parameter,
                value,
                min,
                max,
            } => {
                write!(
                    f,
                    "invalid {}: {} (must be between {} and {})",
                    parameter, value, min, max
                )
            }
        }
    }
}

impl std::error::Error for VBAPError {}

/// Result type alias for VBAP operations.
pub type Result<T> = std::result::Result<T, VBAPError>;
