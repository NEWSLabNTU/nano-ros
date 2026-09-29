//! Put the PAC's `device.x` where `cortex-m-rt`'s `link.x` will find it.
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

fn main() {
    nros_build_paths::link_script!("device.x");
}
