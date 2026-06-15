//! Placeholder entry point — wired up in phase-251 P3 (reporter + CLI).
//! P1 ships the library (model + collectors); the bin target only needs to
//! exist and compile so the crate builds as a workspace member.

fn main() {
    eprintln!("nros-build-profile: CLI lands in phase-251 P3 (reporter + args)");
    std::process::exit(2);
}
