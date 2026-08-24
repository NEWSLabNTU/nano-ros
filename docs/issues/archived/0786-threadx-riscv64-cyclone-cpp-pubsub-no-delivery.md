---
id: 786
title: "`test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub` ran a five-day-old
  binary: two tests hand-joined a fixture path and skipped the staleness probe"
status: resolved
type: bug
area: testing, threadx
related: [issue-0664, issue-0215, issue-0482, phase-376]
resolved_in: "phase-378"
---

## Resolution — it was never a code regression

**The image was five days old.** The C listener rebuilt at 2026-08-24 23:19; both
C++ listeners sat at 2026-08-19 13:34. Rebuilt fresh, all four
`threadx_riscv64_qemu` tests pass in ~1.4 s.

The cause is in the TEST, not the runtime:

```rust
let talker_bin = root.join("examples/qemu-riscv64-threadx/cpp/talker/build-cyclonedds/cpp_talker");
```

A hand-joined path skips both things the resolver exists to do — the lane
coordinate check (`fixtures::lane::require_in_lane`) and the freshness probe
(`require_prebuilt_binary_fresh_cmake`). The tier-2 build lane is 1-wise, so it
need not rebuild this coordinate; the artifact from an older lane simply sat
there and RAN. Per issue 0482 an in-lane fixture that is stale must fail HARD and
an out-of-lane one must SKIP — this did neither, because it never reached the
code that decides.

Fixed by adding `build_rv64_cmake_example_rmw` (the C/C++ resolvers only ever
spelled `build-zenoh`, which is why the test hand-rolled a cyclonedds path in the
first place) and routing both the C and the C++ pubsub tests through it. The
now-redundant `.exists()` guards are deleted: the resolver's own policy is
strictly better — tier-aware skip in the light lane, a `.build-failed` marker
distinguishing "never built" from "build FAILED", and a hard error otherwise.

Swept the class: four more sites in `native_api.rs` hand-joined
`examples/threadx-linux/**/build-cyclonedds/*` and guarded with `.exists()` — a
museum binary satisfies that check. One of them carries a comment describing
issue **0215**, which is this same defect biting in 2026. Those now resolve
through a new `build_threadx_cmake_example_rmw`.

Verified by holding out artifacts rather than by reasoning: fresh → PASS; a
backdated binary → FAIL in 0.18 s naming the binary, the newer source and the
remedy; a removed binary → `[SKIPPED]` under `NROS_FIXTURES_OPTIONAL=1` and a
hard error without it.

## What the original report got wrong

Everything below this line is the diagnosis as first filed. It is kept because
the reasoning was plausible and wrong in an instructive way: a stale artifact
presents as a runtime hang, and both candidate causes named below are real
changes in the right window that had nothing to do with it. **Read the artifact
mtimes before bisecting a hang.** The zenoh C++ image hanging identically was the
tell — one bug in two backends is usually one binary that was not rebuilt.

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
