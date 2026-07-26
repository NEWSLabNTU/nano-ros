---
id: 286
title: "metadata-mode probe can't run for board targets without a cargo `runner` (nuttx lane link-fails on std)"
status: resolved
type: bug
area: cli
related: [0276, 0288]
resolved_in: "issue-0286 (probe_blocker + unsupported degradation)"
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

## Correction — the runner was not the real discriminator (2026-07-26)

The diagnosis above is half right. It correctly identifies that the probe's
target is a property of the component, and that pinning `--target <host>`
alone cannot be the whole answer. But the `runner`/no-`runner` split is NOT
what separates the two lanes.

A concurrent session added the `--target <host>` pin anyway, together with a
`deploy_bound` skip (phase-308) that routes self-contained standalone examples
to `unsupported` before they are ever probed. That combination fixed
qemu-arm-baremetal — verified: `just qemu build-fixtures` is rc=0, and it is
NOT skipped, so it keeps its exact sizing.

nuttx still failed, with a DIFFERENT error than the one recorded above:

```
error[E0599]: no function or associated item named `default` found for
              struct `timespec`
error: could not compile `std` (lib) due to 3 previous errors
Error: refresh source metadata for `nuttx_listener`
```

Not a link failure against the board libc — a failure to rebuild `std` itself.
The actual blocker is that **`[unstable] build-std` is not target-scoped**.
`examples/qemu-arm-nuttx/*/.cargo/config.toml` sets
`build-std = ["std", "panic_abort"]` and points `libc` at a NuttX-patched copy
(`scripts/build/nuttx-libc-patch.sh`, phase-214.M). Cargo therefore rebuilds
`std` from source for whatever it builds — including the harness under
`--target <host>` — against that patched libc, which does not carry the
members host `std` needs.

## Resolution (2026-07-26)

`probe_blocker()` (`orchestration/metadata_build.rs`) reads the cargo config
governing the component and reports `BuildStdForForeignTarget` when
`[unstable] build-std` is set alongside a non-host `[build] target`.
`refresh_stale_sidecars` routes that to `report.unsupported` instead of
failing — the degradation path the module already documents:

> No harness is buildable. Not an error — a sidecar-less bake falls back to
> the SystemModel bound — but never silently pretend the sidecar is current.

The same path already handles deploy-bound crates and non-Rust components, so
this is an additional reason to degrade, not a new mechanism.

Deliberately narrow: a foreign target WITHOUT build-std stays probeable
(`--target <host>` covers it, and skipping would silently cost that lane its
exact executor sizing); build-std targeting the host is not foreign; an
undeterminable host skips nothing, because attempting and failing loudly beats
silently under-counting every executor (the 0257 failure mode).

Receipts, both lanes rebuilt after a CLI rebuild: nuttx `build-fixtures`
rc=2 → **rc=0** with zero std-compile errors; qemu `build-fixtures` **rc=0**
and still probing.

Tests: 6 in `probe_blocker_tests` — the nuttx shape, the baremetal shape,
build-std-for-host, no-target, unknown-host, nearest-config-wins.

## Relationship to #288

#288 (self-contained standalone examples cannot be metadata-probed) is the
same family — un-probeable component, same `unsupported` fallback — but a
different cause (board-crate coupling, no host build) and it is already
handled by the phase-308 `deploy_bound` skip. This issue does not close it;
what remains there is whether those examples should regain exact sizing by
some other route.
