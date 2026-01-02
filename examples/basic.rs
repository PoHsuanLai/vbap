use vbap::VBAPanner;

fn main() {
    let panner = VBAPanner::builder().stereo().build().unwrap();

    // pan left
    let gains = panner.compute_gains(30.0, 0.0);
    println!("L={:.2} R={:.2}", gains[0], gains[1]);

    // pan center
    let gains = panner.compute_gains(0.0, 0.0);
    println!("L={:.2} R={:.2}", gains[0], gains[1]);
}
