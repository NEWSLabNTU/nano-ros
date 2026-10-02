---
id: 1645
title: "C++ images link a message-FFI Rust staticlib beside libnros_cpp.a, duplicating the Rust runtime closure — the last reason for --allow-multiple-definition"
status: open
type: tech-debt
area: build, zephyr, nuttx
severity: low
found: 2026-10-02
related: [1636, 1618, 0734, phase-251]
---

## What

Every C++ image that uses a message package links the generated
`nano_ros_cpp_ffi_<pkg>` crate as its OWN cargo `staticlib`
(`libnano_ros_cpp_ffi_std_msgs.a`), beside `libnros_cpp.a`. A staticlib
bundles its whole closure, so the two archives each carry `compiler_builtins`,
`core`, `alloc` and LLVM `anon.*` constants.

Measured by issue 1636, building `examples/zephyr/cpp/talker` (zenoh) with
`--allow-multiple-definition` removed:

| image | duplicate symbols | from |
| --- | ---: | --- |
| native_sim/native/64 | 392 | all in `libnano_ros_cpp_ffi_std_msgs.a` vs `libnros_cpp.a` |
| mps2/an385 | 571 | same |

All of them are toolchain or runtime internals. No nano-ros C-ABI symbol, no
`REGISTRY` and no zenoh-pico state is among them. So this is the benign half
of the multi-staticlib class, and today it is the only reason the flag stays
in `zephyr/CMakeLists.txt` (C++ API). `integrations/nuttx/Make.defs` and
`integrations/s32ds/makefile.defs` link the same shape, plus `libnros_c.a`
beside `libnros_cpp.a` in the NuttX shell. Neither can be built on the host
that measured this, so both stay allowlisted unverified.

## What closing needs

One Rust staticlib per image: the message FFI reaches the runtime crate as an
RLIB (or the runtime and FFI are bundled by one generated staticlib crate).
Then remove the three allowlist rows in `scripts/allow-multiple-def-allowlist.txt`.
`check-no-allow-multiple-def` fails until each row is lowered.
