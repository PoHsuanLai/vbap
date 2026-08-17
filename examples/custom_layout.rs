use vbap::{PanCursor, VBAPanner};

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

    // `compute_active_gains` hands back only the speakers that receive signal,
    // which is exactly what this display needs. The cursor lets a sweeping
    // source skip the base search while it stays in the same pair.
    let mut cursor = PanCursor::default();

    for deg in (-180..=180).step_by(15) {
        let active: Vec<String> = panner
            .compute_active_gains(deg as f64, 0.0, &mut cursor)
            .iter()
            .filter(|(_, gain)| *gain > 0.001)
            .map(|(speaker, gain)| {
                let bar: String = "█".repeat((gain * 20.0) as usize);
                format!("{} {bar} {gain:.2}", labels[speaker as usize])
            })
            .collect();

        println!("{deg:>4}°  {}", active.join("  "));
    }
}
