---
id: 286
title: "metadata-mode probe can't run for board targets without a cargo `runner` (nuttx lane link-fails on std)"
status: open
type: bug
area: cli
related: [0276]
---

## Finding (full `just build-test-fixtures` sweep, 2026-07-26)

The phase-307 metadata probe (`orchestration/metadata_build.rs`) generates a
harness crate that depends on the component crate plus `nros` with
`features = ["std"]`, then `cargo run`s it so the binary prints the component's
source metadata.

Since the W1 fix the harness runs with `current_dir(harness_dir)` — inside the
consuming workspace, so its `[patch]` entries are in scope. That also brings
the example's `.cargo/config.toml` into scope, including its `[build] target`.

For **qemu-arm-baremetal** this is exactly right and load-bearing: the config
sets `target = "thumbv7m-none-eabi"` AND a runner

```toml
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -semihosting-config enable=on,target=native -kernel"
```

so `cargo run` cross-compiles for the board and executes the probe under QEMU,
reporting back over semihosting. Component crates that hard-depend on a board
crate (`nros-board-mps2-an385` has Cortex-M inline asm — `in("r1")` is not a
valid register on x86_64) can ONLY be built this way.

For **qemu-arm-nuttx** the same mechanism has no exit. The config sets
`target = "armv7a-nuttx-eabihf"` with no runner, and the std-linked probe fails
at link:

```
undefined reference to `malloc'
undefined reference to `pthread_mutex_init'
undefined reference to `_Unwind_Backtrace'
error: could not compile `nros-metadata-probe` (bin "probe")
Error: refresh source metadata for `nuttx_rs_service_server`
```

This blocks `just build-test-fixtures` on the nuttx lane, and therefore
`just ci`.

## Why the obvious fix is wrong

Passing an explicit `--target <host triple>` to pin the probe to the host was
tried and reverted: it fixes nuttx but breaks qemu-arm-baremetal, whose
component crate cannot compile for the host at all. The probe's target is not a
free choice — it is a property of the component crate, and the two lanes want
opposite answers.

## Direction

The probe needs a per-target answer, not a global one. Options, roughly in
increasing cost:

1. **Give nuttx a runner.** If a nuttx image can be run under QEMU the same way
   the baremetal lane is, the existing mechanism just works and this becomes a
   config gap rather than a code change.
2. **Pick the target from the component.** Build host-side when the component
   crate is host-compilable and only fall back to the configured board target
   (with its runner) when it isn't — the probe would need to know which case it
   is in, e.g. from the board/platform recorded in the manifest.
3. **Stop running the probe for no-runner targets** and derive that component's
   metadata another way, failing loudly rather than link-erroring.

## Adjacent fix already landed

The dep-key half of the same sweep's breakage is fixed (`23512f4e7`): the
harness keys its path dependency by the rustc-visible crate name but cargo
resolves path deps by PACKAGE name, so hyphenated examples
(`qemu-rtic-action-client` → crate `qemu_rtic_action_client`) failed with
"no matching package named …". The harness now emits cargo's
`package = "…"` rename read from the component's own manifest.
