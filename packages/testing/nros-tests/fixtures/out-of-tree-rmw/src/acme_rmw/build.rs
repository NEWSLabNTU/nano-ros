//! The `acme` backend reads its own configuration rows (RFC-0140 D5): the
//! knobs `nros-knobs.toml` declares, with the value the image stated, become
//! `$OUT_DIR/nros_knobs.rs`, which `src/lib.rs` includes.
fn main() {
    nros_knob_build::emit(env!("CARGO_MANIFEST_DIR"));
}
