---
id: 23
title: LTO disabled workspace-wide so the opaque-size probe can read symbol byte sizes
status: open
type: tech-debt
area: build
related: [phase-87, phase-118]
---

The release profiles force `lto = "off"` purely so the `nros-sizes-build` opaque-size
probe works, costing binary size on every embedded target.

```toml
# Cargo.toml
[profile.release]
# `lto = true` makes rustc emit each rlib's codegen units as LLVM bitcode
# .rcgu.o members. The `nros-sizes-build` probe parses rlib members as ELF
# objects via the `object` crate, which cannot read bitcode, so every
# `__NROS_SIZE_*` symbol comes back as 0 — see Phase 89.3.
lto = "off"          # Cargo.toml:624
...
[profile.nros-fast-release]
lto = "off"          # Cargo.toml:637
```

## Mechanism

`nros::sizes` (`export_size!`) emits, per opaque handle type:
- `pub static __NROS_SIZE_<NAME>: [u8; size_of::<T>()]` — a static whose **byte
  size** equals the type size.
- (Phase 77.25) `pub fn __nros_size_<NAME>::<const N>() -> usize` + a `#[used]`
  fn-pointer static monomorphised at `N = size_of::<T>()`, so the **symbol name**
  (v0-mangled) encodes the size — e.g. demangles as
  `nros::sizes::rmw_sizes::__nros_size_PUBLISHER_SIZE::<48>`.

`nros-sizes-build::extract_sizes` (`packages/core/nros-sizes-build/src/lib.rs:626`)
reads the **byte-size** path: parse the rlib as an `ar` archive with the `object`
crate, iterate defined symbols, record `ObjectSymbol::size()`. nros-c / nros-cpp
build scripts use the recovered sizes to size `alignas(…) uint8_t storage_[…]`
opaque buffers in generated headers.

**Why LTO breaks it:** under `lto = "fat"/"thin"` rustc emits each rlib codegen
unit as an **LLVM-bitcode** `.rcgu.o`. The `object` crate is an ELF/Mach-O/PE
parser; it cannot read symbol byte sizes from bitcode, so every `__NROS_SIZE_*`
size comes back `0`. The nested probe build inherits the consumer's `PROFILE`
(`lib.rs:263` — `if profile == "release"` adds `--release`), so enabling LTO
poisons the probe itself, not just the final binary.

## Impact

Every release/embedded build runs with `lto = "off"`, leaving binary-size (and some
perf) on the table on space-constrained MCUs — solely a probe limitation, not a
correctness requirement. The size values themselves are LTO-independent
(`size_of::<T>()` is fixed by the target triple's data layout).

## Fix directions

The two are independent; B is the complete fix.

**A — decouple the probe build from the consumer's LTO (surgical).** Sizes are
layout-determined, so the probe rlib need not be LTO'd. Force the nested probe
invocation `lto = off` regardless of the consumer profile:
```rust
// nros-sizes-build, on the nested `cmd`
cmd.env("CARGO_PROFILE_RELEASE_LTO", "false");
```
Then re-enable `lto = "fat"` on the real profiles — the firmware binary is LTO'd,
the probe rlib is native objects. **Caveat:** the filesystem-fallback path
(`NROS_SIZES_PROBE_MODE=filesystem`, used for custom-target JSON specs like
`armv7a-nuttx-eabihf`) reads the *outer* LTO'd rlib and still breaks; keep those
targets `lto=off` or use B.

**B — name-based reader (LTO-agnostic; realizes the Phase 77.25 markers).** Switch
`extract_sizes` from `ObjectSymbol::size()` to reading the v0-mangled marker symbol
**names**. The ar archive symbol *index* lists symbol names even for bitcode
members, and `__nros_size_<NAME>::<N>` encodes the size in the name. Read the names
(`object`'s `ArchiveFile` symbol table, or `llvm-nm` — already in
`rust-toolchain.toml`'s `llvm-tools`), `rustc-demangle`, extract `N`. Works under
fat/thin LTO **and** `lto=off`; fixes **both** probe paths incl. the fallback; lets
every `lto=off` pin be removed. This is the "future sizes-probe that reads bitcode
directly" the `Cargo.toml` comment foreshadows.

**Recommendation:** ship B (removes the LTO constraint everywhere); optionally keep
A's decoupling as belt-and-suspenders. Verify a `lto=fat` build round-trips the
sizes (probe output matches the `lto=off` baseline) before removing the pins.

## Workarounds today

- Builds simply run `lto=off`; correctness is unaffected.
- Per-binary opt-in `lto` already works for smoke/fixture crates that do **not**
  consume the probe (e.g. `nros-smoke/*`, several `logging-smoke-*` set `lto="fat"`)
  — only the probe-consuming graph (nros-c / nros-cpp) needs the pin.

## References

- `Cargo.toml:612-638` — the `lto = "off"` pins + the explanatory comment.
- `packages/core/nros-sizes-build/src/lib.rs` — probe (`extract_sizes` @ 626;
  nested-build profile inherit @ ~263).
- `packages/core/nros/src/sizes.rs` — `export_size!` (byte-size static + Phase
  77.25 v0-mangled marker fn).
- External: rust-lang/rust#66961 (bitcode in rlibs), rustc book "Codegen Options" /
  "Linker-plugin-based LTO", rustc-dev-guide "Libraries and metadata".
