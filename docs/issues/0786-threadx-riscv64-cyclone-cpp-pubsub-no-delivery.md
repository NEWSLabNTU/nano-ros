---
id: 786
title: "`test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub`: the image boots and
  the listener starts, then nothing is delivered — 30 s timeout"
status: open
type: bug
area: testing, threadx
related: [issue-0664, phase-376]
---

## Symptom

```
cargo nextest run -p nros-tests --test threadx_riscv64_qemu --retries 0 \
  -E 'test(cyclonedds_two_qemu_cpp_pubsub)'

FAIL [ 30.079s] nros-tests::threadx_riscv64_qemu
                test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub
```

The image gets a long way in — this is not a boot or link failure:

```
[board] BSD sockets initialized
[app_define] App thread created, returning to kernel...
[virtio] init complete
[virtio] enable: link UP
[app_thread] Calling c_app_main (FFI)...

nros C++ Listener
===================
```

…and then nothing until the 30 s timeout. Sockets are up, virtio link is UP,
the C++ entry runs and prints its banner. Delivery is what is missing.

**Reproduces SOLO**, so it is not the full-sweep QEMU flake class — retested on
its own per the standing rule before filing.

## Not a flake, and a regression

Issue 0664 (resolved 2026-08-17) closes with "All four `threadx_riscv64_qemu`
tests pass (C, C++, Rust)". So this worked a week ago and does not now. It was
found by the first tier-2 run in a while — tier 1 is host-only and cannot see it.

Note the other test in this binary, `test_threadx_riscv64_errno_is_per_thread`,
also shows FAIL under a bare `cargo nextest`; that one is a `nros_tests::skip!`
panic, which only `just test-all`'s junit rewrite scores correctly. Tier 2's own
accounting says **1** real failure, and it is the pubsub cell.

## Candidate window

Commits since 2026-08-17 touching Cyclone, ThreadX or the RV64 board:

- `5932f7233 chore: bump cyclonedds to the Zephyr atomics fix` — moves the
  cyclonedds SUBMODULE pin. A ddsrt change lands under every platform that
  shares that code, so "the Zephyr atomics fix" is not by itself evidence that
  ThreadX is untouched.
- phase-376 W3/W4 (`c7fdc1eb1`, `936e8b4db`, `d1b41b915`, …) — the vtable ABI
  lost its vendor prefix, sixteen slots were renamed and six added. That reaches
  every RMW backend, and a wrong or missing slot on ONE target is exactly the
  shape that boots fine and then delivers nothing.

Bisect on this target is minutes per step, so it wants doing deliberately rather
than opportunistically.

## Explicitly NOT the cause

The `NROS_CYCLONE_HAS_STD_ATOMIC_I64` work (`82d66f45e`, `d233c1746`) is in this
window and is not it:

- on riscv64 that predicate resolves to the NON-atomic path, which is
  byte-identical in selection to what the pre-`b5182d5b3` platform test chose,
  so the preprocessed TU on this target is what it always was;
- it lives in `service.cpp`, which is services — not on the pubsub delivery path
  this test exercises.

What those commits did change is that this cell COMPILES again on riscv64;
between `b5182d5b3` and them it could not build at all, so the test could not
have run to report this.

## Next step

Bisect `5932f7233` against the phase-376 W3/W4 range with this single test, and
check the C and Rust cells of the same binary — if C passes and C++ fails, the
vtable/ABI hypothesis is the stronger one.
