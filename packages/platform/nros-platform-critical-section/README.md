# nros-platform-critical-section

Thin shim crate that registers a global `critical_section::Impl`
backed by the canonical `nros_platform_critical_section_*` C symbols
(Phase 121.9).

Pulls in zero arch-specific code — the actual interrupt-disable /
recursive-mutex body lives in whichever platform port the binary
links (FreeRTOS Cortex-M PRIMASK, Zephyr `irq_lock`, POSIX recursive
pthread mutex, ThreadX `tx_interrupt_control`, etc.).

Replaces the per-arch `critical-section` features on
`nros-platform-{freertos,…}`. Depending on this crate is the single
step needed for any binary that needs `critical_section::with()` —
DDS, nros-rmw-{xrce,zenoh}, embassy-sync, etc.
