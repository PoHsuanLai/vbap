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

/// Error returned by the real-time gain computation path.
///
/// Deliberately separate from [`VBAPError`]: this type is `Copy` and carries no
/// heap-allocated payload, so returning it from an audio callback allocates
/// nothing. [`VBAPError::InvalidConfiguration`] holds a `String` and is confined
/// to the configuration-building path, where allocation is fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanError {
    /// The output slice is smaller than the number of speakers.
    ///
    /// The gains are left untouched — a partial write would silently drop a
    /// speaker's gain and produce a wrong mix.
    BufferTooSmall {
        /// Number of speakers in the configuration.
        need: u32,
        /// Length of the slice that was passed in.
        got: u32,
    },
}

impl fmt::Display for PanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PanError::BufferTooSmall { need, got } => {
                write!(f, "gains slice too small: {} < {}", got, need)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PanError {}

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
