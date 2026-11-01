//! phase-527 fixture command: prints its arguments unchanged (RFC-0119 D7).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("hello {}", args.join(" "));
}
