// Bridge Zephyr Kconfig values into Rust `cfg` flags. Required for
// the zephyr-lang-rust integration path used by `qemu_cortex_a9` /
// any embedded Zephyr Rust target (Phase 92.4). The pattern is
// taken straight from `modules/lang/rust/samples/philosophers`.

fn main() {
    zephyr_build::export_bool_kconfig();
}
