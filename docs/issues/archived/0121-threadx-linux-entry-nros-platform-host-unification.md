---
id: 121
title: "`workspace-rust-threadx-linux` E0463 (`nros_platform` rlib not produced) — was target-dir pollution, NOT feature unification"
status: resolved
type: bug
area: build
related: [phase-267, 0120]
resolved_in: "investigation (not-a-bug on clean build)"
---

## Resolution — not reproducible on a clean build; no code defect

The hypothesised mechanism (Cargo feature unification forcing
`nros-platform[platform-threadx]` onto the x86_64-host `nros` build, leaving no usable
`nros_platform` rlib) is **disproven**. On a **cyclonedds-provisioned pristine `build-test-fixtures`**
(`target-fixtures/threadx-linux` freshly deleted), the leaf builds green:

```
== threadx_linux == OK
```

Direct repro is green every time too — `nros ws sync` (cyclonedds present) + clean target +
`cargo build -p threadx_linux_entry --target x86_64-unknown-linux-gnu` → `nros-platform` compiles
with `platform-threadx` **and** `nros` links its `pub use nros_platform::{…}` re-exports on the host.
So `nros-platform[platform-threadx]` **does** produce a usable host rlib.

**Actual cause: target-dir pollution.** The E0463 only appeared when `target-fixtures/threadx-linux`
held **mixed-flag artifacts** — a `--target x86_64…` build and a no-`--target` build interleaved into
the one dir (accumulated by ad-hoc/manual `cargo build` invocations during investigation). rustc was
then handed an `--extern nros_platform=<path>.rlib` for an artifact incompatible with the current
`nros` compilation → "can't find crate". A `rm -rf target-fixtures/threadx-linux` + rebuild clears it.

**No CI pollution vector exists.** The shared-target-dir resolver
(`NROS_FIXTURE_SHARED_PLATFORMS` in `scripts/build/fixtures-target-dir.sh`) covers only
`qemu-arm-baremetal stm32f4` — `threadx-linux` is not in it, so the fixture harness **and** the
test-all staleness probe both build this row into its own `target-fixtures/threadx-linux` with the
same `--target x86_64-unknown-linux-gnu`. No two automated steps write mismatched `nros_platform`
artifacts into the shared dir. The earlier "cannot reproduce" note on #120 was correct after all;
this split-out was chasing a pollution artifact.

No code change. If it ever recurs, the fix is `rm -rf` the fixture target-dir (or `cargo clean`),
not a change to `nros` / `nros-platform` platform-feature gating.
