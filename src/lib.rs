//! # VBAP - Vector Base Amplitude Panning
//!
//! A Rust implementation of the Vector Base Amplitude Panning (VBAP) algorithm
//! for spatial audio positioning in multichannel speaker setups.
//!
//! VBAP positions sound sources by distributing audio energy across 2-3 adjacent
//! speakers, creating the perception of a phantom sound source at the desired location.
//!
//! ## Features
//!
//! - **2D Panning**: Horizontal-only panning using speaker pairs
//! - **3D Panning**: Full spatial panning using speaker triplets (auto-detected)
//! - **Presets**: Common configurations (stereo, 5.1, 7.1, Atmos, etc.)
//! - **Builder API**: Fluent interface for custom speaker layouts
//! - **SIMD Optimized**: Uses `glam` for fast vector math
//!
//! ## Quick Start
//!
//! ```rust
//! use vbap::VBAPanner;
//!
//! // Create a stereo panner
//! let panner = VBAPanner::builder()
//!     .stereo()
//!     .build()
//!     .unwrap();
//!
//! // Compute gains for a source 15° to the left
//! let gains = panner.compute_gains(15.0, 0.0);
//! println!("L: {:.2}, R: {:.2}", gains[0], gains[1]);
//! ```
//!
//! ## Custom Speaker Layouts
//!
//! ```rust
//! use vbap::VBAPanner;
//!
//! let panner = VBAPanner::builder()
//!     .add_speaker(30.0, 0.0)   // Front Left
//!     .add_speaker(-30.0, 0.0)  // Front Right
//!     .add_speaker(0.0, 0.0)    // Center
//!     .add_speaker(110.0, 0.0)  // Surround Left
//!     .add_speaker(-110.0, 0.0) // Surround Right
//!     .build()
//!     .unwrap();
//!
//! let gains = panner.compute_gains(45.0, 0.0);
//! ```
//!
//! ## 3D Panning (Height Speakers)
//!
//! ```rust
//! use vbap::VBAPanner;
//!
//! // Atmos 7.1.4 layout with height speakers (3D auto-detected)
//! let panner = VBAPanner::builder()
//!     .atmos_7_1_4()
//!     .build()
//!     .unwrap();
//!
//! // Elevated source (45° azimuth, 30° elevation)
//! let gains = panner.compute_gains(45.0, 30.0);
//! ```
//!
//! ## Angle Conventions
//!
//! - **Azimuth**: 0° = front center, 90° = left, -90° = right, 180° = rear
//! - **Elevation**: 0° = horizontal, 90° = above, -90° = below
//!
//! ## References
//!
//! Based on Ville Pulkki's VBAP algorithm:
//! - Pulkki, V. (1997). "Virtual Sound Source Positioning Using Vector Base Amplitude Panning"
//! - Implementation adapted from Ardour DAW's panner code

pub mod config;
pub mod error;
pub mod math;
pub mod panner;
pub mod presets;
pub mod speaker;

// Re-exports for ergonomic API
pub use config::{
    Dimension, InverseMatrix, PanningMode, SpeakerConfig, SpeakerConfigBuilder, SpeakerTuple,
};
pub use error::{Result, VBAPError};
pub use panner::VBAPanner;
pub use speaker::Speaker;
