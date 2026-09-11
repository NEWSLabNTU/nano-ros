# Troubleshooting

**Moved.** The troubleshooting guide now lives in the book:
[Troubleshooting](../../book/src/user-guide/troubleshooting.md).

This file is kept only so that older links do not 404 — the root `README.md`
points here. The book page carries every section this one did (the
`Z_FEATURE_INTEREST` multi-client failure, zenoh-pico version skew, native_sim
`--seed` collisions, TAP setup, the message-too-large buffer ladder, build
issues, hanging tests, the Rust/C FFI pointer-stability trap, the zenoh-pico
error-code table) and two things this one lacks: the Cyclone DDS
two-native_sim-nodes discovery case, and the current `zephyr-workspace`
location.

Three corrections worth knowing if you found this file from an old bookmark:

- The transport limits were shown as `ZPICO_FRAG_MAX_SIZE=… cargo build
  --features rmw-zenoh,platform-posix`. RMW and platform are no longer cargo
  features a user picks; they come from the leaf's `system.toml`
  ([RFC-0098](../design/0098-generated-leaf-build-config.md)). Set the knob in
  the environment and run `nros build` — a value set in the calling environment
  wins over the generated `[env]` table, which deliberately never uses
  `force = true`.
- `cargo build` on its own was given as the rebuild verb. Run `nros sync` first;
  without it the generated message crates and `build/<image>/nros-cargo.toml`
  do not exist.
- It told you to `rm -rf build/` before a `west build`. That is the
  documented antipattern: it destroys the only reproduction of whatever wedged
  the build and proves nothing but that a full build works. Re-configure
  instead — `cmake <build-dir>` regenerates `build.ninja` from the cached
  settings, and `just reconfigure-stale` (contributor) does it across the tree.
  `west build --pristine` is the supported spelling when a Zephyr build really
  must start clean, and the two documented exemptions where a wipe is the only
  option (issue 0834's sizes-header mirror, and a core-crate / `repr(C)` change
  mixing pre- and post-append objects) are the exceptions that prove it.

## See Also

- [Troubleshooting](../../book/src/user-guide/troubleshooting.md) — the live page
- [embedded-tuning.md](embedded-tuning.md) — what each transport knob costs in RAM
