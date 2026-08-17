use vbap::{PanCursor, VBAPanner};

fn main() {
    // 5.1 surround: L(30°), R(-30°), C(0°), Ls(110°), Rs(-110°)
    let panner = VBAPanner::builder().surround_5_1().build().unwrap();

    let sample_rate = 44100u32;
    let duration_secs = 4.0;
    let total_samples = (sample_rate as f64 * duration_secs) as usize;

    // Rotate source 360° around the listener
    let sweep_hz = 0.5; // one full rotation over 2 seconds (2 rotations total)

    let spec = hound::WavSpec {
        channels: 6, // L, R, C, LFE, Ls, Rs (standard 5.1 order)
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let path = "surround_sweep.wav";
    let mut writer = hound::WavWriter::create(path, spec).unwrap();

    // Simple LCG random number generator
    let mut rng_state: u64 = 67890;
    let mut noise = || -> f64 {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng_state >> 33) as f64 / (u32::MAX as f64 / 2.0) - 1.0
    };

    // VBAP gain indices: 0=L, 1=R, 2=C, 3=Ls, 4=Rs
    // WAV 5.1 channel order: L, R, C, LFE, Ls, Rs
    let gain_to_wav = [0, 1, 2, usize::MAX, 3, 4]; // MAX = LFE (silent)

    // Reused across the whole render so the sample loop never allocates —
    // the same pattern an audio callback would use.
    let mut gains = vec![0.0; panner.num_speakers()];
    let mut cursor = PanCursor::default();

    for i in 0..total_samples {
        let t = i as f64 / sample_rate as f64;
        let sample = noise();

        // Rotate azimuth from 0° → 360° (front → left → rear → right → front)
        let azimuth = (t * sweep_hz * 360.0) % 360.0;
        // Convert 0..360 to -180..180 range
        let azimuth = if azimuth > 180.0 {
            azimuth - 360.0
        } else {
            azimuth
        };

        // Scatter into channel order for the interleaved WAV frame.
        gains.fill(0.0);
        panner
            .compute_gains(azimuth, 0.0, &mut cursor)
            .accumulate_into(&mut gains);

        for &idx in &gain_to_wav {
            let value = if idx == usize::MAX {
                0i16 // LFE channel — silent
            } else {
                (sample * gains[idx] * i16::MAX as f64) as i16
            };
            writer.write_sample(value).unwrap();
        }
    }

    writer.finalize().unwrap();
    println!("Wrote {path} ({duration_secs}s, 5.1 surround, noise rotating 360°)");
    println!("Play in a 5.1 setup or inspect in a DAW to hear the source orbit the listener.");
}
