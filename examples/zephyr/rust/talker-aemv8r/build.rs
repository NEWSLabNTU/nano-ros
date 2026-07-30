// phase-291 (#211) — the shared zephyr-leaf build plumbing:
// - `export_kconfig_bool_options()` — Kconfig→cfg bridge (phase-92.4
//   silent-boot guard; pattern from modules/lang/rust/samples/philosophers).
// - `bake_nros_config()` — the known-issue #17 locator/domain bake + the
//   issue-0163 XRCE agent-locator synthesis (canonical implementation in
//   `packages/core/nros-zephyr-build`; see its docs for the full rationale).
fn main() {
    zephyr_build::export_kconfig_bool_options();
    nros_zephyr_build::bake_nros_config();
}
