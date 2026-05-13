# Phase 123.A.1 — Binary self-containedness audit

**Date:** 2026-05-13
**Subject:** Whether the installed `libnros_c.a` / `libnros_cpp.a`
archives can be redistributed as decoupled core / platform / RMW
static libraries per the Stream A design.

**Verdict:** Today they cannot. Archives are monolithic — each
per-(RMW, platform) `.a` embeds zenoh-pico (or equivalent) source
files + Rust compiler_builtins + nros core + all dependencies.
Decoupling requires changing the Cargo build to emit **three
separate `staticlib` crates**, not a single combined one.

The platform-cffi + RMW-cffi vtables (canonical C ABI) are already
referenced inside the archive — the **API contract is decoupled**.
The **physical packaging** is not.

## Method

Inspected the installed artefacts under `build/install/lib/`
using `ar t`, `nm`, `objdump`, `strings`, and `strip`. Archives
audited: `libnros_c_zenoh.a` (~28 MB) + `libnros_c_dds.a`
(~26 MB) on `x86_64-unknown-linux-gnu`, built with
`-DNANO_ROS_RMW={zenoh,dds} -DNANO_ROS_PLATFORM=posix`.

## What's inside `libnros_c_zenoh.a`

468 object files total. Typology:

| Origin | Count | % of objs | Role |
|---|---|---|---|
| `compiler_builtins-*.o` | 265 | 57% | Rust intrinsics (memcpy, atomics, panic infra) |
| zenoh-pico C objects (`*.c.o`) | 126 | 27% | Entire upstream zenoh-pico C library statically embedded |
| `nros-*.o` (Rust crates) | 13 | 3% | Actual nano-ros C-FFI shim + executor + bindings |
| `core-*`, `std-*`, `alloc-*` | 6 | 1% | Rust standard library |
| `zpico-sys-*` | 3 | <1% | nano-ros's bindgen + cffi to zenoh-pico |
| misc (panic_abort, etc.) | 55 | 12% | runtime support |

Disk-size proxy: 28 MB total. ~70% is Rust compiler_builtins,
~25% is zenoh-pico, ~3% is actual nano-ros code. **The archive
is not decoupled.**

`libnros_c_dds.a` shows the same shape with dust-DDS C sources
instead of zenoh-pico, and ~210 nros symbols common with the
zenoh archive (the rest are post-build-time skew — dds archive
predates Phase 122.3 work).

## Vtable references

The canonical C ABI work (Phase 115.M, 121, platform-cffi /
RMW-cffi) is wired:

- `nros_rmw_cffi::CffiSession`, `CffiPublisher`, `CffiSubscriber`,
  `CffiServiceClient`, `CffiServiceServer` — referenced via
  mangled Rust symbol names. Vtable dispatch active.
- `Session::create_publisher`, `Publisher::publish_raw`,
  `Drop::drop` — undefined references resolved at link time
  against `nros_rmw_cffi`'s Rust trait impls.

So **the contract is decoupled**: a hypothetical separately-built
`libnros_core.a` could reference the vtable symbols and a
separately-built `libnros_rmw_zenoh.a` could supply them.
Today's archive happens to bundle them, but that's a Cargo
build-output shape, not an ABI constraint.

## Undefined symbols (what the user's linker must already have)

A clean set, no surprises:

- **libc** (POSIX target): `malloc`, `free`, `memcpy`, `memcmp`,
  `clock_gettime`, `getrandom`, `fcntl`, sockets API (`accept`,
  `bind`, `connect`, `listen`, …).
- **libpthread**: implied by `pthread_*` symbols (resolved at
  CMake link time — `NanoRosCppTargets.cmake` adds
  `pthread dl m` for the POSIX combo).
- **No source-build artefacts.** Verified with `nm` — no
  `_RUSTC_PATH_*` placeholder, no rustc-rmeta sections, no
  Cargo-local crate-IDs in the public ABI.

C++ archive has the same undefined-symbol set + libstdc++
ABI stubs (`__cxa_*`, vtable typeinfo). No surprise.

## Source-path leakage

`strings` against the unstripped archive turns up paths that
match the host that built it:

```
/home/aeon/.cargo/registry/src/index.crates.io-…/heapless-0.8.0/src/vec.rs
/home/aeon/.rustup/toolchains/stable-x86_64-…/lib/rustlib/src/rust/library/std/src/…
/home/aeon/repos/nano-ros/build/cmake-zenoh/cargo/…/zpico-sys-…/out/zenoh-pico-build/src/api/admin_space.c
```

Origins:

1. **Panic messages.** Rust's `core::panicking::panic_fmt` bakes
   the file path of the originating `.rs` line into the panic
   string. Visible via `strings` until stripped.
2. **Debuginfo.** `cargo build --release` defaults to
   `debug = 0` for our crates but transitive deps may carry
   line-number metadata. Many path strings come from DWARF
   `.debug_*` sections.
3. **`zpico-sys` C-build directory.** The `cc` build script
   compiles zenoh-pico C with `-g` + relative `-I` flags that
   bake the absolute `OUT_DIR` path.

**Mitigation — measured.** Running `strip --strip-debug` on
`libnros_c_zenoh.a` cuts size from **28 MB → 16 MB (-43%)** and
reduces visible path strings from "many" to **12** (the
remaining are panic strings from the Rust core, which need
`--remap-path-prefix=$HOME/.cargo=cargo` style rustc flags to
fully erase).

For a redistributable SDK the recommended pipeline is:

```bash
RUSTFLAGS="--remap-path-prefix=$HOME=. --remap-path-prefix=/tmp=."
cargo build --release
strip --strip-debug build/install/lib/libnros_*.a
```

Halves on-disk size and removes nearly all environmental leakage.

## Conclusions

### Decoupling state

| Layer | Decoupled today? | Notes |
|---|---|---|
| **API contract** (platform / RMW vtable) | ✅ yes | platform-cffi + RMW-cffi vtables wired |
| **Physical archive** (`.a` content) | ❌ no | Cargo's `staticlib` crate-type bundles all transitive deps |
| **Undefined-symbol layer** (libc, pthread) | ✅ yes | Resolves at user's link time, as expected |
| **Source-path reproducibility** | ⚠ partial | `--remap-path-prefix` + `strip --strip-debug` needed |

### Implications for Stream A

1. **Three-archive split requires Cargo build-output changes.**
   The nros-c, nros-cpp, nros-rmw-* and nros-platform-* crates
   today depend on each other through `[dependencies]` and emit
   one big `staticlib`. To produce three separate `.a` files:
   - Make `nros-c` (and `nros-cpp`) build with **no RMW or
     platform features active**. Result: a small archive
     (~few MB without compiler_builtins, larger with) that
     references vtable symbols as undefined.
   - Build `nros-platform-posix` (and `nros-platform-zephyr`,
     etc.) as their own `staticlib` crate-type. Each ships
     the platform-cffi vtable implementation for its target.
   - Build `nros-rmw-zenoh` (and `nros-rmw-xrce`, etc.) as
     their own `staticlib`. Each ships the RMW-cffi vtable
     impl + the bundled transport library (zenoh-pico,
     micro-XRCE-DDS-Client, …).
2. **`compiler_builtins` is per-archive.** A `staticlib` ships
   its own copy because Cargo can't share it across separate
   builds. So three archives = three copies of
   compiler_builtins (~5 MB each). Acceptable cost for the
   matrix collapse.
3. **Cross-archive Rust types still need a shared `nros-core`
   rlib reference.** When `libnros_rmw_zenoh.a` references
   `nros_rmw_cffi::CffiSession` (Rust mangled name) it must
   agree byte-for-byte with the definition in
   `libnros_c.a`. Same Cargo.lock + same rustc nightly =
   agreement (already pinned).
4. **Source-path scrubbing is a release-pipeline step,
   not a build-output change.** `RUSTFLAGS=--remap-path-prefix`
   + post-build `strip --strip-debug` get applied in CI before
   the install layout is published. Add to A.3 (or
   future split-archive CI).

### Recommendations

- **Add an A.1.x sub-item:** "Cargo build-output refactor for
  three separate `staticlib` crates." Order it before A.3
  (`nros setup` CLI fetch logic — the CLI assumes the
  three-archive layout exists in the install).
- **Add a release-time `--remap-path-prefix` + `strip` step**
  to `installation.md` recommendations + the cmake install
  rules.
- **Accept the per-archive `compiler_builtins` cost** (15 MB
  per archive). Document in the SDK matrix.
- **Decoupled-then-recombined alternative (rejected):** Keep
  one big archive but parameterise by Cargo feature at user's
  install time. Users on RTOS targets can't run `cargo build`
  themselves (the whole point of source distribution is that
  they can — see the source-ship decision). So we either ship
  the decoupled archives or ship source. The "one big
  parameterised archive" doesn't help.

### Open follow-ups

- Concretely refactor `packages/core/nros-c/Cargo.toml` to drop
  the RMW / platform feature flags from its own dep graph, and
  carve `nros-rmw-zenoh-staticlib` / `nros-platform-posix-staticlib`
  wrapper crates. Mechanical but touches every variant of every
  `just <plat> install` recipe.
- Verify the `staticlib` linker can resolve cross-archive Rust
  symbol references without an rlib intermediate. (Rust's
  `staticlib` includes all transitive dep symbols by default;
  the question is whether linking three `staticlib`s together
  produces ODR violations on common symbols like
  `compiler_builtins` — probably yes, requires `--allow-multiple-definition`
  or compiler_builtins ABI version-locking.)
- Measure end-to-end SDK download size under the decoupled
  layout: core (~8 MB stripped) + platform-posix (~3 MB) +
  rmw-zenoh (~12 MB with zenoh-pico) = ~23 MB per
  (target, platform, rmw) — compared to today's monolithic
  16 MB stripped. Per-target cost goes up slightly but matrix
  size collapses dramatically (next sub-item).

## Audit data

Inspection commands captured at:

```
/home/aeon/repos/nano-ros/build/install/lib/libnros_c_zenoh.a   (28 MB)
/home/aeon/repos/nano-ros/build/install/lib/libnros_c_dds.a     (26 MB)
```

Reproduction:

```bash
ar t build/install/lib/libnros_c_zenoh.a              | wc -l    # → 468
nm build/install/lib/libnros_c_zenoh.a | grep ' T z_' | head     # zenoh-pico symbols defined
strings build/install/lib/libnros_c_zenoh.a | grep /home/aeon    # host-paths leaked
cp build/install/lib/libnros_c_zenoh.a /tmp/x.a && strip --strip-debug /tmp/x.a
ls -la /tmp/x.a                                                  # → 16 MB
```
