//! Error types for VBAP operations.

use alloc::string::String;
use core::fmt;

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
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VBAPError {}

/// Result type alias for VBAP operations.
pub type Result<T> = core::result::Result<T, VBAPError>;
