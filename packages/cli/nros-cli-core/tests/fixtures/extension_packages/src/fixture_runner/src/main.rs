//! phase-527 fixture runner: answers `describe` (RFC-0119 D6) and refuses
//! everything else with the runner-failed code.
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("describe") => {
            println!(r#"{{"protocol":1,"verbs":["run","console"],"needs":["elf"]}}"#)
        }
        _ => std::process::exit(2),
    }
}
