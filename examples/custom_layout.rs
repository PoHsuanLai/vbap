use vbap::VBAPanner;

fn main() {
    // Build a custom 5-speaker layout: an asymmetric setup
    let panner = VBAPanner::builder()
        .add_speaker(0.0, 0.0) // Front Center
        .add_speaker(50.0, 0.0) // Front Left (wider than usual)
        .add_speaker(-40.0, 0.0) // Front Right
        .add_speaker(130.0, 0.0) // Rear Left
        .add_speaker(-120.0, 0.0) // Rear Right
        .build()
        .unwrap();

    let labels = ["FC", "FL", "FR", "RL", "RR"];

    println!("Custom 5-speaker layout: FC(0°) FL(50°) FR(-40°) RL(130°) RR(-120°)");
    println!();

    for deg in (-180..=180).step_by(15) {
        let gains = panner.compute_gains(deg as f64, 0.0);

        // Show only active speakers
        let active: Vec<String> = labels
            .iter()
            .zip(gains.iter())
            .filter(|(_, &g)| g > 0.001)
            .map(|(label, &g)| {
                let bar_len = (g * 20.0) as usize;
                let bar: String = "█".repeat(bar_len);
                format!("{label} {bar} {g:.2}")
            })
            .collect();

        println!("{deg:>4}°  {}", active.join("  "));
    }
}
