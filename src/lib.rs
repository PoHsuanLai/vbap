#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod config;
pub mod error;
pub mod math;
pub mod panner;
pub mod presets;
pub mod speaker;

// Re-exports for ergonomic API
pub use config::{
    Dimension, PanningMode, SpeakerConfig, SpeakerConfigBuilder, SpeakerPair, SpeakerTriplet,
};
pub use error::{PanError, Result, VBAPError};
pub use panner::{ActiveGains, PanCursor, VBAPanner};
pub use speaker::Speaker;
