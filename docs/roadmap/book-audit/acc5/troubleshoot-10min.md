BLOCKERS
None. Every `nros setup`, `just <plat> doctor` and PATH/store path the page recommends resolves and runs; the page meets the post-D+E bar.

FRICTION
1. C/C++ transport-failure branch (lines 58–62) cites wrong identifiers and exit codes: `nros_init` doesn't exist (C talker uses `nros_support_init`); `ConnectionFailed` maps to `NROS_RET_NOT_FOUND = -4` (not `-3`), and `NROS_CHECK_RET(call, 1)` makes the process exit `1`, not `-3`/`-100`. The `-100` for C++ is the internal `NROS_CPP_RET_TRANSPORT_ERROR` but isn't the *process* exit.
2. CMake "missing nros codegen" decision branch (lines 30–38) keys on strings (`` `nros-codegen` not found `` / `failed to find tool. Is nros installed?`) that **the build never prints** — actual literal is `nros (codegen tool) not found on PATH or in ~/.nros/bin`. A user searching by error string won't hit this branch.
3. `just doctor tier=default` (page recommendation line 87) reports `[MISSING] cargo-tools: nros` even when `~/.nros/bin/nros 0.3.7` is installed and working — the doctor probes the in-tree `cargo install` path only. Users sent here loop on reinstall.
4. `NROS_LOCATOR` (canonical override the talkers actually read first) is never mentioned — only `ZENOH_LOCATOR` appears.

CLARITY
- Symptom-mapping sections CLEAR in shape; specific strings/IDs stale (see FRICTION above).

MISSING STEPS
- No `direnv allow` hint (would catch the `FREERTOS_PORT not set` panic from `zpico-sys/build.rs`).
- No Cyclone-DDS hint that `cargo build --features rmw-cyclonedds` cannot link at all (Phase 175 requires the CMakeLists path).

WORKS
- All `nros setup …` board recipes.
- `just freertos/nuttx doctor`.
- zenohd shim path.
- RMW interop instruction.
- All three log-line samples for Rust/C/C++ talker.
- `thumbv7m-none-eabi` cargo target reference.

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: just doctor tier=default
LAST EXIT CODE: 1 (doctor reported stale in-tree probe; tutorial path works regardless)
