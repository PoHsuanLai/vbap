use std::f64::consts::PI;
use vbap::VBAPanner;

fn main() {
    let panner = VBAPanner::builder().stereo().build().unwrap();

    let sample_rate = 44100u32;
    let duration_secs = 4.0;
    let total_samples = (sample_rate as f64 * duration_secs) as usize;

    // Sweep azimuth back and forth between hard left (30°) and hard right (-30°)
    let sweep_hz = 2.0; // complete left-right-left cycles per second

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let path = "panning_sweep.wav";
    let mut writer = hound::WavWriter::create(path, spec).unwrap();

    // Simple LCG random number generator (no extra dependencies)
    let mut rng_state: u64 = 12345;
    let mut noise = || -> f64 {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng_state >> 33) as f64 / (u32::MAX as f64 / 2.0) - 1.0
    };

    for i in 0..total_samples {
        let t = i as f64 / sample_rate as f64;

        // White noise — much easier to localize than a pure tone
        let sample = noise();

        // Ping-pong azimuth: sin oscillates between -1..1, map to -30..30
        let azimuth = 30.0 * (t * sweep_hz * 2.0 * PI).sin();
        let gains = panner.compute_gains(azimuth, 0.0);

        let left = (sample * gains[0] * i16::MAX as f64) as i16;
        let right = (sample * gains[1] * i16::MAX as f64) as i16;

        writer.write_sample(left).unwrap();
        writer.write_sample(right).unwrap();
    }

    writer.finalize().unwrap();
    println!("Wrote {path} ({duration_secs}s stereo, noise sweeping left ↔ right at {sweep_hz} Hz)");
    println!("Play it with headphones to hear VBAP panning!");
}
