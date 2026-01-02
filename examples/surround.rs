use vbap::VBAPanner;

fn main() {
    let panner = VBAPanner::builder().surround_5_1().build().unwrap();

    // rotate around
    for azi in [-180, -90, 0, 90, 180] {
        let gains = panner.compute_gains(azi as f64, 0.0);
        let active: Vec<_> = gains
            .iter()
            .enumerate()
            .filter(|(_, &g)| g > 0.01)
            .collect();
        println!("azi={:4}: {:?}", azi, active);
    }
}
