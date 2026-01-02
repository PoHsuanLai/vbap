use vbap::VBAPanner;

fn main() {
    let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

    println!("speakers: {}", panner.num_speakers());
    println!("mode: {:?}", panner.mode());

    // elevated source
    let gains = panner.compute_gains(45.0, 30.0);
    let active: Vec<_> = gains
        .iter()
        .enumerate()
        .filter(|(_, &g)| g > 0.01)
        .collect();
    println!("active speakers: {:?}", active);
}
