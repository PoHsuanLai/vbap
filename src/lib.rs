#![cfg_attr(not(feature = "std"), no_std)]
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
//! - **`no_std` Compatible**: Works without the standard library (requires `alloc`)
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
//! // Compute gains for a source 15° to the left into a pre-allocated
//! // slice — the alloc-free path suitable for audio threads.
//! let mut gains = vec![0.0; panner.num_speakers()];
//! panner.compute_gains_into(15.0, 0.0, &mut gains);
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
//! let mut gains = vec![0.0; panner.num_speakers()];
//! panner.compute_gains_into(45.0, 0.0, &mut gains);
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
//! let mut gains = vec![0.0; panner.num_speakers()];
//! panner.compute_gains_into(45.0, 30.0, &mut gains);
//! ```
//!
//! ## Angle Conventions
//!
//! - **Azimuth**: 0° = front center, 90° = left, -90° = right, 180° = rear
//! - **Elevation**: 0° = horizontal, 90° = above, -90° = below
//!
//! This follows the counter-clockwise positive convention defined in
//! [ITU-R BS.2076](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2076-2-201910-S!!PDF-E.pdf)
//! (Audio Definition Model) and the
//! [EBU ADM Guidelines](https://adm.ebu.io/reference/excursions/coordinate_system.html):
//! 0° straight ahead, positive azimuth to the left.
//!
//! ## Real-time use
//!
//! On an audio thread, prefer [`VBAPanner::compute_active_gains`]. It returns
//! only the two or three speakers that actually receive signal, so it does no
//! work proportional to the speaker count, and it accumulates into a mix buffer
//! that you clear once per block rather than once per source.
//!
//! ```rust
//! use vbap::{PanCursor, VBAPanner};
//!
//! let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();
//!
//! // One cursor per source; it remembers the last speaker base so a moving
//! // source usually skips the search entirely.
//! let mut cursor = PanCursor::default();
//! let mut mix = vec![0.0; panner.num_speakers()];
//!
//! for (azimuth, elevation) in [(0.0, 0.0), (45.0, 30.0)] {
//!     let active = panner.compute_active_gains(azimuth, elevation, &mut cursor);
//!     active.accumulate_into(&mut mix);
//! }
//! ```
//!
//! Both this and [`VBAPanner::compute_gains_into`] allocate nothing, take no
//! locks, and cannot panic in release builds. `VBAPanner` is `Send + Sync`, so
//! one panner can serve many voices concurrently.
//!
//! ## Coverage
//!
//! VBAP can only place a source inside the region the speakers span (Pulkki
//! §3). Layouts that surround the listener cover every azimuth; a layout with a
//! wide gap — stereo, LCR, a frontal array, or any dome with nothing below the
//! horizon — produces silence for directions outside that region rather than a
//! phantom it cannot render. Inside the covered region the gains always satisfy
//! `Σg² = 1`.
//!
//! ## References
//!
//! - Pulkki, V. (1997). ["Virtual Sound Source Positioning Using Vector Base Amplitude Panning."](https://www.aes.org/e-lib/browse.cfm?elib=7853)
//!   *J. Audio Eng. Soc.*, 45(6), 456–466.

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
