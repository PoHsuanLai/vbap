# vbap

[![CI](https://github.com/PoHsuanLai/vbap/actions/workflows/ci.yml/badge.svg)](https://github.com/PoHsuanLai/vbap/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/vbap.svg)](https://crates.io/crates/vbap)
[![docs.rs](https://docs.rs/vbap/badge.svg)](https://docs.rs/vbap)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)

[Vector Base Amplitude Panning](https://www.audiolabs-erlangen.de/media/pages/resources/aps-w23/papers/935eb793db-1663358804/sap_Pulkki1997.pdf) (VBAP) positions virtual sound sources in a speaker array by computing gain coefficients for the 2-3 speakers nearest to the source direction. Originally described by Ville Pulkki in 1997.

## Usage

```rust
use vbap::VBAPanner;

let panner = VBAPanner::builder()
    .stereo()
    .build()
    .unwrap();

// Write into a buffer you own — no allocation per call.
let mut gains = vec![0.0; panner.num_speakers()];
panner.compute_gains_into(15.0, 0.0, &mut gains); // 15° left
```

## Real-time use

On an audio thread, prefer `compute_active_gains`. It returns only the two or
three speakers that actually receive signal, so it does no work proportional to
the speaker count, and it accumulates into a mix buffer you clear once per block
rather than once per source.

```rust
use vbap::{PanCursor, VBAPanner};

let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

// One cursor per source; it remembers the last speaker base, so a moving
// source usually skips the search entirely.
let mut cursor = PanCursor::default();
let mut mix = vec![0.0; panner.num_speakers()];

let active = panner.compute_active_gains(45.0, 30.0, &mut cursor);
active.accumulate_into(&mut mix);
```

Both entry points allocate nothing, take no locks, and cannot panic in release
builds. `VBAPanner` is `Send + Sync`, so one panner can serve many voices
concurrently. Use `try_compute_gains_into` when the output length is not
statically known.

## Presets

- `stereo()` - L/R at ±30°
- `surround_5_1()` - standard 5.1
- `surround_7_1()` - standard 7.1
- `atmos_7_1_4()` - 7.1.4 with height speakers

## Custom layouts

```rust
use vbap::VBAPanner;

let panner = VBAPanner::builder()
    .add_speaker(30.0, 0.0)   // azimuth, elevation
    .add_speaker(-30.0, 0.0)
    .add_speaker(0.0, 0.0)
    .add_speaker(110.0, 0.0)
    .add_speaker(-110.0, 0.0)
    .build()
    .unwrap();
```

Adding a speaker with non-zero elevation switches the panner to 3D
automatically, using speaker triplets instead of pairs:

```rust
use vbap::VBAPanner;

let panner = VBAPanner::builder()
    .atmos_7_1_4()  // 7.1 base layer plus four height speakers
    .build()
    .unwrap();

let mut gains = vec![0.0; panner.num_speakers()];
panner.compute_gains_into(45.0, 30.0, &mut gains);
```

## Angles

- Azimuth: 0° front, 90° left, -90° right, 180° rear
- Elevation: 0° horizontal, 90° above

This follows the counter-clockwise positive convention defined in:

- [ITU-R BS.2076](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2076-2-201910-S!!PDF-E.pdf) (Audio Definition Model) — 0° front, positive azimuth to the left
- [EBU ADM Guidelines — Coordinate System](https://adm.ebu.io/reference/excursions/coordinate_system.html)

And consistent with the original VBAP paper:

- Pulkki, V. (1997). ["Virtual Sound Source Positioning Using Vector Base Amplitude Panning."](https://www.aes.org/e-lib/browse.cfm?elib=7853) *J. Audio Eng. Soc.*, 45(6), 456–466.

## Coverage

VBAP can only place a source inside the region the speakers span. Layouts that
surround the listener cover every azimuth; a layout with a wide gap — stereo,
LCR, a frontal array, or a dome with nothing below the horizon — produces
silence for directions outside that region rather than a phantom it cannot
render. Inside the covered region the gains always satisfy `Σg² = 1`.

## `no_std` support

This crate works without the standard library. Disable the default `std` feature in your `Cargo.toml`:

```toml
[dependencies]
vbap = { version = "0.2", default-features = false }
```

An allocator is still required (`alloc`). The only difference is that `VBAPError` won't implement `std::error::Error`.

## License

MIT
