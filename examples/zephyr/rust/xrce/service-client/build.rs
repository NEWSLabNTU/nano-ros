// Bridge Zephyr Kconfig values into Rust `cfg` flags. Pattern from
// modules/lang/rust/samples/philosophers — required for embedded
// ARM Cortex-A targets (silent-boot bug if absent — Phase 92.4).
fn main() {
    zephyr_build::export_bool_kconfig();
}
