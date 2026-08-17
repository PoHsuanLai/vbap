# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0]

Gains change for most layouts in this release. Every change is a bug fix — the
previous output violated properties the VBAP paper states explicitly — so there
is no way to opt back into the old behaviour.

Upgrading from 0.1.1, the last published release: 0.1.2 was tagged in the
repository but never released, so its deprecation of `compute_gains` in favour
of `compute_gains_into` arrives here too.

### Fixed

- **The centre channel was silent in every 3D layout.** In Atmos 7.1.4 the centre
  speaker belonged to no speaker triplet, so a source panned straight ahead came
  out of the left speaker at 0.894 instead (30° off-axis, 1 dB down). Atmos 5.1.4
  was worse: L, R and C were all unreachable. Triplet selection now uses the
  convex hull of the speaker directions, which by construction tiles the sphere
  exactly once — no orphaned speakers, no overlapping active regions.

  Triplet counts change accordingly: 7.1.4 `10 → 13`, 5.1.4 `8 → 11`,
  9.1.6 `→ 19`, Auro 9.1 `→ 11`.

- **Open 2D layouts silenced the speakers they should have used.** A wrap-around
  pair was formed unconditionally, and on a layout that does not surround the
  listener its active arc also covered the front, where it won speaker selection.
  In `lcr()` the centre speaker was silent across the whole front and jumped
  discontinuously (0.45 at −20°, 0.0 from −10° to +10°, 0.45 at +20°) — an
  audible click when panning. A 5-speaker frontal array never used its 0°
  speaker at all. The ring is now closed only when the remaining gap is narrow
  enough to be a valid pair.

- **Negative gain factors were clamped after normalization instead of before**,
  contrary to Pulkki §1.4. Out-of-arc directions lost up to 7 dB
  (−3.01 dB at 90° on a stereo pair). Inside the active region this was always a
  no-op, so closed layouts are unaffected.

- **Sources outside the region the speakers span now produce silence** rather
  than a renormalized full-level phantom, per Pulkki §3.

- **Speaker azimuths are normalized to `[-180, 180)`.** Unwrapped angles such as
  370° sorted to the wrong position and destroyed the adjacency structure that
  pair selection depends on.

- **`Dimension::Force2D` no longer mixes elevated speakers into pairs.** Pair
  validity was gated on the full 3D angle while the basis matrix used azimuth
  alone; both now use azimuth.

- The triplet degeneracy threshold rejected geometry the paper validates in §6.2
  (a 5°/175°/175° triangle). Superseded by a test for a basis plane passing
  through the listener, which is the actual singularity.

- Dense speaker rings build again — a 100-speaker ring at 3.6° spacing was
  rejected outright by a 5° minimum-separation floor.

- `stereo()` produced the same pair twice.

### Added

- `VBAPanner::try_compute_gains_into`, returning `Result<(), PanError>` for a
  too-small output slice. `PanError` is `Copy` and allocation-free, so it is safe
  to use on an audio thread.
- Non-finite (`NaN`/infinite) input is now *guaranteed* to yield all-zero gains
  rather than leaking `NaN` into the output buffer.

### Changed

- **`compute_gains_into` no longer panics in release builds.** The length check
  is now a `debug_assert!`; unwinding out of an audio callback is undefined
  behaviour under most host ABIs. Use `try_compute_gains_into` when the length is
  not statically known.
- Better error messages when no valid bases can be formed — in particular for a
  fully horizontal layout built with `Dimension::Force3D`.

### Removed

- `math::lines_intersect` (crate-internal). It reported arcs sharing an endpoint
  as intersecting, and the edge-pruning stage it fed was the direct cause of the
  orphaned centre channel.
