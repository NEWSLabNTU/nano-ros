---
id: 1147
title: "`mem-report` counts the C++ executor storage but files it under the `nros`
  RUST crate, so the C/C++ half of every embedded image's largest pool is
  attributed to the wrong owner and can never join a declared pool"
status: resolved
resolved: 2026-10-01
type: tech-debt
area: tooling
related: [phase-392, 0815]
---

## Problem

`scripts/nros-mem-report.py` attributes a symbol to a crate lexically, on the
first `lowercase_ident::` of the DEMANGLED name:

```python
CRATE = re.compile(r"\b([a-z][a-z0-9_]*)::")
```

`nm -C` demangles Itanium C++ as well as Rust v0, so the C++ executor storage
— `Node::GlobalStorageHolder<0>::storage`, an `alignas(8) uint8_t
[NROS_CPP_EXECUTOR_STORAGE_SIZE]` in `.bss` (`packages/api/nros-cpp/include/nros/node.hpp`)
— arrives as `nros::Node::GlobalStorageHolder<0>::storage` and matches `nros`.
That is the name of a **Rust crate in this workspace** (`packages/api/nros`), so
the bytes land in its bucket, indistinguishable from Rust `nros` statics.

Pool attribution then cannot reach it either. The join key is
(crate ident, last `::` segment), and it additionally requires
`name.startswith(crate + "::")` where `crate` comes from the declaring file's
`Cargo.toml` name. A pool would have to be declared in a crate literally named
`nros` and named `storage` — and `gen-pool-inventory.py` scans tracked `*.rs`
only (`git ls-files '*.rs'`), so a `// nros-pool:` comment in the C++ header is
invisible to it regardless.

The plain C path is worse: a file-scope `static struct { … nros_executor_t
executor; … } app;` has no `::` at all, so it falls into the
`(C / asm / no path)` bucket. On the native Rust zenoh talker that bucket is
91,216 bytes; on a C or C++ image it also holds the executor storage.

## Why it matters now

phase-392 W6 made the RUST arm visible (`nros_node::executor::backing::EXECUTOR_BACKING`,
21,560 B measured on the native talker). The C and C++ arms were already in
`.bss` — they were never the invisible ones — but they are **mis-attributed**,
and the embedded images are exactly the C/C++ ones. So the campaign can now
price this pool on the platform where it matters least and not on the platforms
it was opened for.

## Not "just add a mapping"

The tempting fix — special-case `nros::Node::GlobalStorageHolder` — is the
authored-map drift class this tree already has scars from (the RMW parity map,
CLAUDE.md). Two shapes worth considering instead:

1. **Give the storage an unmangled name.** Define it once in Rust
   (`#[unsafe(no_mangle)] static mut nros_cpp_executor_storage`) and have
   `node.hpp` declare it `extern "C"`. That removes the C++ template-static
   COMDAT trick, removes the `NROS_CPP_EXECUTOR_STORAGE_SIZE` header/Rust size
   mirror, and gives one greppable symbol across every C/C++ image. It also
   inherits `NROS_EXECUTOR_BACKING_SECTION` for free. Cost: it touches
   `nros_cpp_init`'s storage parameter and the NuttX `__cxa_guard` workaround
   the template-static exists to dodge (`node.hpp` documents that empirically),
   so it needs a NuttX build to accept.
2. **Teach the report a C++ arm.** Attribute a demangled `A::B::C` with no Rust
   crate of that name to the DECLARING header's component. Cheaper, but it
   invents a second attribution rule, and the mis-attribution to a real Rust
   crate would still need a tiebreak.

Whichever is chosen, the acceptance is the phase's standing rule: a BUILT C or
C++ image showing the storage under an owner a reader would guess, confirmed
with `just mem-report`.

## Related

- phase-392 W1 (issue 0815) — the instrument this is about.
- phase-392 W6 — fixed the Rust arm and measured it; this is the arm it left.

## Resolution

Neither shape exactly; the report learned two things, each bound to the tree.

**The language comes from the MANGLING** (`lang_of`), read before anything is
demangled: `_R…` and `_ZN…17h<hash>E` are Rust, any other `_Z…` is C++,
the rest C. The crate rule runs on Rust symbols only, so a C++ name — even one
spelled `nros::…`, the exact collision reported here — is filed under
`C++ <namespace>`, never under the Rust crate `nros`. That is the "tiebreak"
option 2 lacked, and it needs no per-symbol mapping.

**Storage has an owner of its own** (`STORAGE_ROLES`): `[executor storage]`
for `__nros_executor_storage`, `__nros_tier_executor_storage`,
`Node::GlobalStorageHolder<N>::storage` (any namespace), the Rust
`EXECUTOR_BACKING` and `__NROS_TIER_EXECUTOR_BACKING`; `[component storage]`
for `__nros_comp_buf_N` and `__NROS_COMPONENT_<pkg>_SLOT_STORE`. This is an
authored table, so the always-on selftest binds it both ways
(`check_storage_roles`): every row's defining literal must still be in the file
it cites, every tracked line that DEFINES such storage must be in a cited file,
and every cited file must still contain a definition — each direction mutation
-tested by dropping each row in turn (all seven fail).

**The C/C++ storage is PRICED, not just named**: the report reads the build's
own sizes header (nearest build tree only — one level further up is a west
workspace's sibling builds, which the first draft wrongly read and refused on
"headers disagree") and prints `N x NROS_CPP_EXECUTOR_STORAGE_SIZE`, refusing
with the reason when the header is newer than the image or two disagree, and
printing MISMATCH when the measured size is not a whole number of executors.

Option 1 (an unmangled Rust-defined symbol) was not needed for attribution and
was not done; its other benefits (dropping the size mirror, the NuttX
`__cxa_guard` dodge) stand on their own.

### Measured — three tiered images built from this branch

| image | executor storage | priced | component storage |
| --- | --- | --- | --- |
| FreeRTOS mps2-an385 C (`workspace-c-freertos-realtime`) | `__nros_tier_executor_storage` 178,144 | `2 x 89,072` | — |
| Zephyr native_sim C++ (`workspace-zephyr-cpp-derived-tiers`) | `__nros_tier_executor_storage` 95,904 | `4 x 23,976` | `__nros_comp_buf_0..3` 4 x 1,176 |
| native Rust (`workspace-rust-native-realtime`) | `EXECUTOR_BACKING` 88,552 + `__NROS_TIER_EXECUTOR_BACKING` 88,560 | measured (no header states the Rust slot) | `*_SLOT_STORE` 2 x 4,032 |

Before, on the earlier build of the same Zephyr row (another checkout, 2026-09-30): `__nros_tier_executor_storage` and the four
component buffers sat in `(C / asm / no path)` (513,326 B, 144.3 % of a section
total that was itself wrong — issue 1606); after, `[executor storage]` 95,904
and `[component storage]` 4,704 are their own owners. On the Rust image the
tier backing had been attributed to the user's ENTRY crate (`native_entry`) and
the boot backing to `nros_node`; both are `[executor storage]` now (177,112 B).

### Not measured

No image here contains `Node::GlobalStorageHolder<0>::storage` — every C++
image built from this branch takes the entry's tier/executor table, so that
row is covered by the selftest (`rclcpp::…` and `nros::…` spellings) and the
source binding, not by an image.
