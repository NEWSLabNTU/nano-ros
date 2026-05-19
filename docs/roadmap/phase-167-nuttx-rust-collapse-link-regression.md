# Phase 167 — NuttX Rust collapse: libgloss / newlib crt0 link regression

**Goal.** Restore `cargo build` on the collapsed-shape
`examples/qemu-arm-nuttx/rust/<case>/` directories (Phase 118.B.5
target) so the per-RMW feature-gated Rust examples link cleanly
alongside the working C / C++ siblings. Today the Rust collapse
fails at link time with `undefined reference to __libc_init_array
/ __libc_fini_array` (libgloss crt0); the historical
`examples/qemu-arm-nuttx/rust/zenoh/<case>/` siblings (one
directory deeper) link cleanly with the same toolchain, same
`.cargo/config.toml`, and same Cargo dep graph.

**Status.** Not Started — investigation aborted in Phase 118.B.5
after the surface-level diffs (Cargo.toml, `.cargo/config.toml`,
`rust-toolchain.toml` discovery, workspace.exclude membership)
all came back identical between the working depth-5 layout and
the failing depth-4 layout.

**Priority.** P2 — the C / C++ side of NuttX is fully collapsed
(12 / 12 binaries built + `phase_118_collapse` smokes green); the
Rust side is the only remaining blocker for closing Phase 118.B.5
end-to-end. Until then NuttX Rust stays on the legacy
`<plat>/<lang>/<rmw>/<case>/` shape and the Phase 118 matrix lint
will flag NuttX Rust as legacy-shape while every other (plat,
lang) cell reports collapsed-shape.

**Depends on.** Phase 118 (example-matrix collapse mechanism),
specifically:
- 118.A.1 — Rust collapse PoC on `native/rust/talker/` proved the
  `optional dep + rmw-* feature` shape on the Rust side.
- 118.B.4 — FreeRTOS Rust collapse landed cleanly with the same
  shape (single feature axis `rmw-{zenoh,dds}` + per-RMW
  `--target-dir target-<rmw>/`).
- 118.B.5 — NuttX C / C++ collapse landed but the Rust side hit
  this link issue and was reverted from the branch.

**Related.** `examples/qemu-arm-nuttx/rust/zenoh/talker/`
(working baseline), `examples/qemu-arm-nuttx/rust/dds/talker/`
(working baseline), `third-party/nuttx/libc/` (the patched libc
crate this issue points at), `.cargo/config.toml` `[patch.crates-io]
libc = { path = ... }` stanza.

---

## Symptom

When the collapsed talker is placed at
`examples/qemu-arm-nuttx/rust/talker/` (depth 4 under
`examples/`) and built with:

```
cd examples/qemu-arm-nuttx/rust/talker
CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
  cargo build --release --no-default-features --features rmw-zenoh \
              --target-dir target-zenoh
```

the link step fails:

```
/usr/lib/gcc/arm-none-eabi/10.3.1/../../../arm-none-eabi/bin/ld:
  /build/newlib-pB30de/newlib-3.3.0/build/arm-none-eabi/thumb/v7ve+simd/hard/
   libgloss/arm/semihv2m/../../../../../../../../libgloss/arm/crt0.S:541:
  undefined reference to `__libc_init_array'
…ld: …crt0.S:547: undefined reference to `__libc_fini_array'
```

The same command in the historical-shape sibling
(`examples/qemu-arm-nuttx/rust/zenoh/talker/`, depth 5)
succeeds without any code or config change:

```
cd examples/qemu-arm-nuttx/rust/zenoh/talker
CC_armv7a_nuttx_eabi=arm-none-eabi-gcc cargo build --release
    Finished `release` profile [optimized] target(s) in 12s
```

The C / C++ siblings at the collapsed depth
(`examples/qemu-arm-nuttx/c/talker/`, also depth 4) build cleanly
via `cmake -DNROS_RMW=zenoh && cmake --build build-zenoh -j` —
the issue is Rust-side only.

## What's been ruled out

- **Cargo.toml content.** Byte-equivalent after the
  `../../../../../` → `../../../../` path-depth shortening that the
  collapse refactor requires. Both `[dependencies]`,
  `[profile.release]`, and `[features]` lists match across the
  working and failing trees.
- **`.cargo/config.toml`.** Same byte-equivalent after the same
  path-depth shortening. `[unstable] build-std`, `[target.armv7a-
  nuttx-eabihf]` linker / rustflags, `[env]` block, and the
  `[patch.crates-io] libc = { path = "...third-party/nuttx/libc" }`
  patch all read identically. The path resolves to the same
  absolute libc directory from both depths
  (`realpath ../../../../third-party/nuttx/libc` vs
  `realpath ../../../../../third-party/nuttx/libc`).
- **`rust-toolchain.toml` discovery.** The example pins
  `nightly-2026-04-11` via
  `examples/qemu-arm-nuttx/rust-toolchain.toml` (one level above
  `<rmw>/<case>/`). Cargo's parent-walk finds it from both depths
  (`rustup show active-toolchain` reports the same toolchain in
  both example dirs).
- **Cargo workspace membership.** Adding the collapsed dir to
  `[workspace.exclude]` in the root `Cargo.toml` and removing it
  both produce the same link error. Workspace boundary
  discovery is the same in both cases.
- **Toolchain env (`CC_armv7a_nuttx_eabi`, `CFLAGS_...`).** Same
  shell invocation pattern; both examples are built with the
  same env vars.

## What's left to investigate

1. **`build-std`'s libc resolution scope.** The nuttx examples
   compile std-from-source via `[unstable] build-std = ["std",
   "panic_abort"]`. The patched libc must replace the upstream
   libc inside std's build. The patch is applied via the example
   dir's `.cargo/config.toml`. Cargo's `[patch.crates-io]`
   resolution scope might be sensitive to where the
   `Cargo.toml` lives relative to the patched path target —
   verify with `cargo build -v ... 2>&1 | grep 'rustc.*libc'` on
   both depths and compare which libc rlib gets pulled into the
   std build.
2. **`crt0.S` from libgloss.** The error is from libgloss
   (newlib's startup), not from the patched NuttX libc. So either
   (a) std's build-script is preferring libgloss because the
   patched libc didn't replace the toolchain crate, or (b) the
   linker is pulling crt0 from `arm-none-eabi-gcc`'s default
   search path even though `-nostartfiles` / `-nodefaultlibs` are
   supposed to suppress it. Check whether
   `nros-board-nuttx-qemu-arm`'s build.rs emits those flags only
   on certain Cargo.toml layouts.
3. **`-nostartfiles` / `-nodefaultlibs` emission.** Trace which
   build.rs emits these `cargo:rustc-link-arg=` lines on the
   working build. The board crate's build.rs only prints
   `rustc-link-lib=static=nros_platform_nuttx`. The
   `nros-nuttx-ffi` crate's build.rs (which DOES emit
   `-nostartfiles` / `-nodefaultlibs`) is a cmake-only crate and
   isn't pulled by the pure-Rust example. So the working build
   must get these flags from somewhere else — likely a
   `[target.armv7a-nuttx-eabihf]` stanza in a `.cargo/config.toml`
   higher up the tree, or via the target spec. Worth dumping
   `rustc --target armv7a-nuttx-eabihf --print target-spec-json`
   to confirm `pre-link-args` / `post-link-args` setup.

## Reproducer

The current `phase-118-example-matrix-collapse` branch contains
the collapse machinery + the failing helper:

```
git checkout phase-118-example-matrix-collapse
tmp/collapse-nuttx-rust-case.sh talker
cd examples/qemu-arm-nuttx/rust/talker
CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
  cargo build --release --no-default-features \
              --features rmw-zenoh --target-dir target-zenoh
# → undefined reference to __libc_init_array
```

The control (works):

```
cd examples/qemu-arm-nuttx/rust/zenoh/talker
CC_armv7a_nuttx_eabi=arm-none-eabi-gcc cargo build --release
# → Finished
```

## Files (when 167.1 lands)

- `examples/qemu-arm-nuttx/rust/<case>/Cargo.toml` (six cells)
- `examples/qemu-arm-nuttx/rust/<case>/.cargo/config.toml`
- `examples/qemu-arm-nuttx/rust/<case>/src/main.rs`
- Possibly `packages/boards/nros-board-nuttx-qemu-arm/build.rs`
  (if the fix is to emit additional `cargo:rustc-link-arg=`
  directives that the legacy 5-segment build was getting
  implicitly from somewhere else)
- `just/nuttx.just` (extend `build-fixtures` to drive the
  collapsed Rust cases alongside C / C++)
- `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
  (add `build_nuttx_rust_example_rmw` helper)
- `packages/testing/nros-tests/tests/phase_118_collapse.rs` (add
  `test_nuttx_rust_case_rmw_variant_exists`)
- `tmp/collapse-nuttx-rust-case.sh` (already on the branch —
  produces the failing layout; verify it still emits a working
  one after the underlying fix)

## Acceptance criteria

- [ ] `cargo build --release --no-default-features --features
       rmw-<rmw>` succeeds for every `rmw` listed in each
       collapsed dir's `Cargo.toml`.
- [ ] `just nuttx build-fixtures` produces the collapsed Rust
       binaries at
       `examples/qemu-arm-nuttx/rust/<case>/target-<rmw>/
        armv7a-nuttx-eabihf/release/<binary>` alongside the
       legacy zenoh + dds siblings.
- [ ] `test_nuttx_rust_case_rmw_variant_exists` passes 6+
       parametrized cases.
- [ ] No regression on the legacy
       `examples/qemu-arm-nuttx/rust/{zenoh,dds}/<case>/`
       builds — the fix must not break the historical-shape
       siblings that downstream consumers still reference.

## Notes

- **Why this stops Phase 118.B.5.** The NuttX C / C++ rows are
  already collapsed and green; without 167 the NuttX Rust row
  must stay on the `<plat>/<lang>/<rmw>/<case>/` shape, leaving
  one platform's Rust axis legacy while every other platform's
  Rust axis is collapsed. Phase 118's matrix lint can still
  enforce that NuttX Rust is documented as
  legacy-pending-167-fix; the cell isn't blocked, it's just
  reported differently.
- **The depth-4 vs depth-5 footgun.** The same diff
  (depth 5 → depth 4) on FreeRTOS, ThreadX-Linux (likely),
  ThreadX-RV64 (likely) builds without issue. NuttX is the only
  RTOS that uses `[unstable] build-std` for the Rust examples
  (the others compile against the pre-built `thumbv7m-none-eabi`
  /`armv7a-none-eabi` rust-std rlib). That's the obvious axis
  separating "works" from "doesn't work" — start the
  investigation by validating exactly how `[patch.crates-io]`
  interacts with build-std at different `Cargo.toml` depths.
- **C / C++ stays clean.** The cmake-side build resolves linker
  flags through `add_subdirectory(<repo>)` → `nros_platform_link_app()`
  which is depth-agnostic; nothing in the C / C++ path depends
  on the Cargo workspace boundary.
