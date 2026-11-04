//! phase-527 W4 fixture command: prints its arguments, one per line.
fn main() {
    for a in std::env::args().skip(1) {
        println!("{a}");
    }
}
