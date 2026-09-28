---
id: 1542
title: "An ungoverned `cc::Build` in the baremetal C talker example, on the compiler issue 0478 is about"
status: resolved
type: bug
area: [build, examples]
severity: low
found: 2026-09-28
related: [phase-472, 0478]
---

## What happens

`examples/mps2-an385-baremetal/c/talker/build.rs:94`:

```rust
let mut build = cc::Build::new();
```

builds C with `arm-none-eabi-gcc` outside `nros-cc-flags`, the governed wrapper
every `packages/**` build script goes through. VERIFIED present on `main`.

`check-cc-build-policy` reads `packages/**/*.rs` only, so a copy-out example is
outside its population; the same file copied under `packages/` fails the gate.

## What is NOT established

Whether copy-out examples SHOULD take `nros-cc-flags`. RFC-0026 makes examples
standalone, and a governed dependency may be the wrong shape for them. The answer
is either "adopt the wrapper" or "an exemption with a reason" — not silence.

## Fix

Decide, then either route it through `nros-cc-flags` or give examples a stated
exemption; widen the gate either way (phase-472 W5).

## Resolution

Decided: **adopt the wrapper** (option A). An example's `build.rs` drives the same
cc-rs against the same `arm-none-eabi-gcc` as a board crate, so both classes the
wrapper exists for (0383's implicit declarations, which the pinned gcc 13.2 only
warns on; 0478's clang-only `-mno-omit-leaf-frame-pointer`, which gcc rejects)
reach it identically. A copy-out consumer names `nros-cc-flags` the way it names
`nros-c`: `version = "*"`, patched by `nros sync`.

- `examples/mps2-an385-baremetal/c/talker/build.rs` calls
  `nros_cc_flags::strict_decls(&mut build)` (which also applies
  `gcc_safe_frame_pointer`) BEFORE `build.clone()`, so the generated-bindings
  build inherits it; `warnings(false)` there drops only `-Wall -Wextra`.
- `Cargo.toml` gains `nros-cc-flags = { version = "*" }` under
  `[build-dependencies]`.
- `nros sync` learns the crate: `("nros-cc-flags", "packages/tooling/nros-cc-flags")`
  in `nros_crate_path_lookup()` (`packages/cli/nros-cli-core/src/cmd/ws.rs`),
  beside the `nros-zephyr-build` precedent. Without it sync skips the name as an
  unknown runtime crate and the leaf resolves against crates.io.
- `check-cc-build-policy` population widened from `packages/**/*.rs` to every
  tracked `*.rs` (git pathspec, harvested — no authored list), and the gate now
  runs a selftest on every invocation (bare / governed / doc-comment-only
  matcher cases, plus a reach check that the population contains an
  `examples/**/build.rs`). Removed from `.config/gate-selftest-baseline.txt`.

### Measured

- Widened gate on origin/main's `build.rs`: rc=1, names
  `examples/mps2-an385-baremetal/c/talker/build.rs`. With the fix: rc=0.
- origin/main's gate script on the same unfixed file: rc=0 (the reach gap).
- Mutations of the gate, each confirmed by `diff`: population reverted to
  `packages/**/*.rs` -> selftest fails, rc=1; matcher blinded
  (`if false && ...`) -> selftest fails, rc=1.
- `nros sync` then `cargo build --release` in the leaf: rc=0. The sync writes
  `nros-cc-flags = { path = "../../../../packages/tooling/nros-cc-flags" }  # nros-managed`.
  With `CC_ENABLE_DEBUG_OUTPUT=1`, all 34 C compile lines (the app TU and the
  generated `std_msgs`/`builtin_interfaces` TUs) carry
  `-Werror=implicit-function-declaration -Werror=int-conversion`.

- `CARGO_PROFILE_RELEASE_DEBUG=1` build (so cc-rs forces a frame pointer),
  same leaf, `cc` 1.5.1: fixed `build.rs` rc=0, 34/34 compile lines carry both
  `-Werror=` flags and `-fno-omit-frame-pointer`. CONTROL with origin/main's
  `build.rs`, same settings: rc=0, 0/34 carry the `-Werror=` flags.
  `-mno-omit-leaf-frame-pointer` appears only on cc-rs's own flag-support
  probe in BOTH runs, never on a compile line: cc 1.5.1 already drops it for
  gcc. So on today's lock the strict-declaration half is the load-bearing one
  for this leaf; the 0478 half is a guard against a `cc` that regresses, as it
  was for the workspace in 0478. The leaf's `Cargo.lock` is gitignored
  (`examples/**/Cargo.lock`), so no lock moved.

### NOT verified

- Runtime: this leaf is `BuildOnly` (issue 1512); nothing boots it under QEMU.
- A `cc` version that emits `-mno-omit-leaf-frame-pointer` to gcc was not
  reproduced here, so the 0478 half is exercised only as "flag present, build
  green", not as "fixes a red".
