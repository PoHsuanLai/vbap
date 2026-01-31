use std::f64::consts::PI;
use vbap::VBAPanner;

fn main() {
    let panner = VBAPanner::builder().stereo().build().unwrap();

    let sample_rate = 44100u32;
    let duration_secs = 4.0;
    let tone_hz = 440.0;
    let total_samples = (sample_rate as f64 * duration_secs) as usize;

    // Sweep azimuth from 30° (hard left) to -30° (hard right) over the duration
    let start_azi = 30.0_f64;
    let end_azi = -30.0_f64;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let path = "panning_sweep.wav";
    let mut writer = hound::WavWriter::create(path, spec).unwrap();

    for i in 0..total_samples {
        let t = i as f64 / sample_rate as f64;
        let progress = i as f64 / total_samples as f64;

        // Sine tone
        let sample = (t * tone_hz * 2.0 * PI).sin();

        // Interpolate azimuth
        let azimuth = start_azi + (end_azi - start_azi) * progress;
        let gains = panner.compute_gains(azimuth, 0.0);

        let left = (sample * gains[0] * i16::MAX as f64) as i16;
        let right = (sample * gains[1] * i16::MAX as f64) as i16;

        writer.write_sample(left).unwrap();
        writer.write_sample(right).unwrap();
    }

    writer.finalize().unwrap();
    println!("Wrote {path} ({duration_secs}s stereo, {tone_hz} Hz tone sweeping left → right)");
    println!("Play it with headphones to hear VBAP panning!");
}
