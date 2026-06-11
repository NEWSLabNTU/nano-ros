---
id: 32
title: zephyr fixtures reported stale right after a clean rebuild — dir-mtime false positive
status: resolved
type: bug
area: testing
related: [phase-177, issue-0030]
resolved_in: "path_newer_than dir-mtime fix (packages/testing/nros-tests/src/zephyr.rs)"
---

**Problem.** Every `just test-all` reported ~32 zephyr e2e tests as failures with:

```
Failed to get zephyr-cpp-talker binary: BuildFailed("Zephyr fixture binary is
stale: .../build-cpp-talker-zenoh/zephyr/zephyr.exe
Run `just zephyr build-fixtures` before running Zephyr tests.")
```

— **even immediately after a clean, green `just zephyr build-fixtures`** (all 55
leaves built, exit 0). Rebuilding could never clear it.

**Root cause.** The runtime freshness heuristic `is_binary_stale`
(`zephyr.rs`) is mtime-based: it flags a fixture binary if any *watched* source
path (`examples/zephyr/<lang>/<app>`, `packages/core/*`, `packages/zpico|dds|
xrce`, `zephyr/`, conf files) is newer than the binary. Its helper
`path_newer_than` returned `true` whenever a **directory's OWN mtime** exceeded
the cutoff:

```rust
if meta.modified().is_ok_and(|mtime| mtime > cutoff) { return true; }  // dirs too
if !meta.is_dir() { return false; }
// ...then recurse into entries
```

A directory's mtime bumps on **any transient entry add/remove** — not just real
content changes. The test-all tail step `_test-c-codegen` (c-msg-gen) churns
`packages/core/nros-{c,cpp}/include/nros/` (writes + removes temp files) while
every header inside stays byte-identical (git-clean). That bumped the *dir*
mtime to the run's end-time, so on the next check every zephyr binary built
before it looked stale. Confirmed: `include/nros/` dir mtime `22:15` (the
c-codegen step) vs all files inside `≤14:56` and the binary `21:22`; `git
status` clean.

**Fix.** In `path_newer_than`, only trust **file** mtimes; for directories,
skip the own-mtime check and recurse into entries (real file mtimes reflect
real content changes). Pure deletions aren't mtime-detectable anyway — the
build-side content signature (`.nros-zephyr-fixture.sig`) is the safety net for
those.

```rust
if !meta.is_dir() {
    return meta.modified().is_ok_and(|mtime| mtime > cutoff);  // files only
}
// dirs: recurse; ignore the dir's own (churn-sensitive) mtime
```

**Validated.** After the fix, `test_zephyr_cpp_talker_to_listener_e2e` runs the
real `native_sim` binary and **PASSes** (32.9 s) instead of false-stale-failing
in 0.2 s. Surfaced during a local `just test-all` shake-out (2026-06-11); same
run also fixed the standalone esp32-baremetal `.bss` overflow
([issue 0024](0024-esp32-dram-overflow-size-class-buffers.md) follow-up).
