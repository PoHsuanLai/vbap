# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0]

Gains change for most layouts in this release. Every change is a bug fix — the
previous output violated properties the VBAP paper states explicitly — so there
is no way to opt back into the old behaviour.

0.1.1 is the last published release; 0.1.2 was committed but never released, so
upgrading skips it. `compute_gains` still exists but has a new signature and
return type — see **Changed** below.

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

### Changed

- **There is now one way to compute gains.** `compute_gains` takes a
  `PanCursor` and returns an `ActiveGains` — the two or three speakers that
  receive signal, each with its gain. It replaces the previous four entry
  points (`compute_gains`, `compute_gains_into`, `try_compute_gains_into`, and
  `compute_active_gains`), which differed only in output shape and in how they
  reported a too-small buffer.

  ```rust
  // before
  let mut gains = vec![0.0; panner.num_speakers()];
  panner.compute_gains_into(45.0, 30.0, &mut gains);

  // after
  let mut cursor = PanCursor::default();
  panner.compute_gains(45.0, 30.0, &mut cursor).accumulate_into(&mut gains);
  ```

  Nothing scales with the speaker count, nothing allocates, nothing locks, and
  nothing panics in release, so the same call serves an audio callback and
  offline rendering. There is no longer a "real-time" variant to choose,
  because the ordinary path already meets that bar.

  Measured against the previous release (2M calls along a moving-source path):
  64-speaker ring `78.7 → 13.4 ns` (5.9x), 128-speaker ring `→ 19.2 ns`,
  Atmos 7.1.4 `30.4 → 17.5 ns` (1.7x). Allocation-free, verified with a
  counting allocator.

- Bases are now stored per mode (`Vec<SpeakerPair>` or `Vec<SpeakerTriplet>`)
  with inline `[u32; N]` indices, instead of one `Vec<SpeakerTuple>` of enums
  each owning a heap-allocated index vector. The panning mode is resolved once
  per call rather than once per candidate base, which also lets the search loop
  vectorize.

- Better error messages when no valid bases can be formed — in particular for a
  fully horizontal layout built with `Dimension::Force3D`.

### Added

- `PanCursor`, which remembers the last selected base so a moving source can
  skip the search while it stays inside that base. Held by the caller rather
  than the panner, which keeps `&VBAPanner` `Sync` — one panner can still be
  shared across voices and threads — and gives each source its own coherence.
  A default or stale cursor only costs speed, never correctness.
- `ActiveGains`, with `iter`, `len`, `is_empty`, and `accumulate_into`. The
  last adds into a channel-indexed buffer, so many sources sum into one buffer
  that the caller clears once per block rather than once per source.
- Non-finite (`NaN`/infinite) input is now *guaranteed* to yield no gains
  rather than leaking `NaN` into the output buffer.
- `SpeakerConfig::pairs`, `SpeakerConfig::triplets`, and
  `SpeakerConfig::num_bases`.

### Removed

- `compute_gains_into`, `try_compute_gains_into`, `compute_active_gains`, and
  the `PanError` type they needed. Gains carry their own speaker index, so
  there is no output-slice length to validate and no error case left to report.

- `SpeakerTuple`, `InverseMatrix`, and `SpeakerConfig::tuples()`, replaced by
  `SpeakerPair`, `SpeakerTriplet`, and the `pairs()`/`triplets()` accessors.
  These were inspection-only surface; a compatibility shim would have had to
  rebuild a `Vec` per call, reintroducing the allocation the change removes.

- `math::lines_intersect` (crate-internal). It reported arcs sharing an endpoint
  as intersecting, and the edge-pruning stage it fed was the direct cause of the
  orphaned centre channel.
