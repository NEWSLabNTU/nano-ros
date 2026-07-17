---
id: 235
title: "threadx-riscv64 C++ CycloneDDS pubsub has no runtime lane — the riscv64 example set builds C + rust cyclone, not C++"
status: open
type: enhancement
area: testing
related: [issue-0233, issue-0214]
---

## Finding (issue #233 cell 5, 2026-07-18)

The `ThreadxRiscv64 · Cpp · Cyclonedds · Pubsub` matrix cell is `BuildOnly`.
Unlike the other #233 cyclone cells, it has NO fixture to wire: the
`examples/qemu-riscv64-threadx/cpp/*` example dirs exist but the
`just threadx_riscv64 build-fixtures` recipe builds only the **C** and
**rust** cyclone two-QEMU images (issue #214), not a C++ cyclone variant.
The runtime lanes that exist are `test_threadx_riscv64_cyclonedds_two_qemu_
pubsub` (C) and `..._rust_pubsub` (rust).

## What it needs (fixture-creation, not just test wiring)

1. Add a C++ cyclone build variant to the threadx-riscv64 fixture recipe
   (`just/threadx-riscv64.just` + `examples/fixtures.toml` rows), mirroring
   the C cyclone build config — the C++ cyclone image must link the
   `descriptors.cpp` type support like the threadx-linux C++ path does.
2. Add `test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub` mirroring the
   existing C/rust two-QEMU lanes (`threadx_riscv64_qemu.rs`): two QEMU
   guests over the `-netdev socket,mcast=` virtual L2, distinct MACs per
   the #214 identity fix, distinct baked domains.
3. Flip the matrix cell to `Runtime`; the fixtures⊆⊇matrix gate then keeps
   the new row honest.

## Note

This is the heaviest of the #233 cells (a new cross-compiled image + a
two-QEMU e2e), which is why it was carved out of the wire-existing-fixture
pass. The C++ cyclone code path itself is proven on threadx-LINUX
(`test_threadx_linux_cyclonedds_cpp_talker_to_native_listener`, #233), so
the risk is confined to the riscv64 build config + QEMU pairing, not the
RMW.
