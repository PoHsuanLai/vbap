# vbap

[![CI](https://github.com/PoHsuanLai/vbap/actions/workflows/ci.yml/badge.svg)](https://github.com/PoHsuanLai/vbap/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/vbap.svg)](https://crates.io/crates/vbap)
[![docs.rs](https://docs.rs/vbap/badge.svg)](https://docs.rs/vbap)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)

[Vector Base Amplitude Panning](https://www.audiolabs-erlangen.de/media/pages/resources/aps-w23/papers/935eb793db-1663358804/sap_Pulkki1997.pdf) (VBAP) positions virtual sound sources in a speaker array by computing gain coefficients for the 2-3 speakers nearest to the source direction. Originally described by Ville Pulkki in 1997.

Inspired by [Ardour](https://ardour.org/)'s implementation.

## Usage

```rust
use vbap::VBAPanner;

let panner = VBAPanner::builder()
    .stereo()
    .build()
    .unwrap();

let gains = panner.compute_gains(15.0, 0.0); // 15° left
```

## Presets

- `stereo()` - L/R at ±30°
- `surround_5_1()` - standard 5.1
- `surround_7_1()` - standard 7.1
- `atmos_7_1_4()` - 7.1.4 with height speakers

## Custom layouts

```rust
let panner = VBAPanner::builder()
    .add_speaker(30.0, 0.0)   // azimuth, elevation
    .add_speaker(-30.0, 0.0)
    .add_speaker(110.0, 0.0)
    .add_speaker(-110.0, 0.0)
    .build()
    .unwrap();
```

## Angles

- Azimuth: 0° front, 90° left, -90° right, 180° rear
- Elevation: 0° horizontal, 90° above

## License

MIT
