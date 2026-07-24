# phase-300 W4 — the ONE cross-toolchain presence predicate (sourced).
#
# Issue 0030's lockstep pair — the workspace-fixture preflight
# (`check-fixtures-stale.sh`) and the `test-all` env_exclude block — each
# hand-coded its own toolchain probe; they must agree or a fixture is
# demanded whose e2e test was filtered out (or vice versa). One definition.
#
# Usage:  source scripts/test/toolchain-gate.sh
#         nros_toolchain_present arm-none-eabi && ...

nros_toolchain_present() {
    case "$1" in
        arm-none-eabi)
            command -v arm-none-eabi-gcc >/dev/null 2>&1 ;;
        riscv64-elf)
            command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 ;;
        threadx)
            [ -n "${THREADX_DIR:-}" ] || [ -d third-party/threadx/kernel ] ;;
        *)
            echo "nros_toolchain_present: unknown toolchain key '$1'" >&2
            return 2 ;;
    esac
}
