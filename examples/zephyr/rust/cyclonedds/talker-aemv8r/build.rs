// Bridge Zephyr Kconfig values into Rust `cfg` flags. Required for
// embedded ARM Cortex-A targets (silent-boot bug if absent — Phase 92.4).
fn main() {
    zephyr_build::export_kconfig_bool_options();
}
