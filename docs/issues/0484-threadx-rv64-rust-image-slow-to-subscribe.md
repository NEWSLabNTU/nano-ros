---
id: 484
title: "ThreadX-rv64 RUST image takes 2.1 s to reach `Subscriber created`, against
  0.1 s for the C and C++ images"
status: open
type: bug
area: threadx
related: [phase-342, issue-0481]
---

## Measurement

Same test file, same QEMU invocation, same CycloneDDS RMW, same host, one run
(`threadx_riscv64_qemu.rs`, box, fixtures freshly built):

| cell | listener-ready | delivery | total |
| --- | --- | --- | --- |
| c | **0.10 s** | 1.10 s | 1.25 s |
| cpp | **0.10 s** | 1.20 s | 1.34 s |
| **rust** | **2.11 s** | 3.11 s | **5.26 s** |

`listener-ready` is the wait for `LISTENER_READY_MARKER`
("Subscriber created for topic:"), printed immediately after the subscription is
created. `delivery` is the subsequent wait for the first sample.

**The Rust image is ~21× slower to reach the same line**, and that delay
propagates: its talker boots on the same path, so the first sample lands ~2 s
later too. The delivery gap is largely the readiness gap paid twice, not a
separate transport problem.

The values are quantised to ~100 ms because `wait_for_output_pattern` polls at
that interval; 2.105 s is ~21 polls, not a coincidence.

## How it surfaced

It did not, for as long as anyone had looked. The test slept a fixed
`Duration::from_secs(4)` after starting the listener — comfortably longer than
either image needed — so C at 0.1 s and rust at 2.1 s were indistinguishable.
phase-342 W8b replaced that sleep with a wait on the readiness marker, and the
per-cell numbers separated immediately.

That is the second time this shape appeared in one phase: splitting the pubsub
fold exposed `rust_cyclone` at 34 s against 5 s siblings (issue 0481). **A fixed
delay does not just cost its duration — it hides the distribution underneath
it.**

## Not yet diagnosed

Deliberately filed on the measurement alone rather than a guess. What is known:

- The rust image is **7,642,992 bytes** against C's **6,533,536** (+17 %), so
  some of it is plausibly QEMU `-kernel` load time — but 17 % of a 0.1 s load
  does not buy 2 s.
- It is NOT a test-harness artifact: the rust test resolves its binaries through
  `build_threadx_rv64_rust_example_rmw`, which only joins a path and checks
  existence — no compile at test time (checked).
- Both use the same `QemuProcess::start_riscv64_virt_{dgram,mcast}` paths and the
  same `-M virt -m 256M -bios none` invocation.

Candidates worth measuring next, cheapest first: time from QEMU start to the
image's FIRST output line (isolates load+boot from nros init); whether
`build-std` / panic-unwind machinery is linked in; whether the Rust CycloneDDS
participant creation does work the C path does lazily.

## Why it matters beyond 4 seconds

This is the tier-2 threadx-riscv64 lane, and the same rust-vs-C asymmetry would
appear on any timing-sensitive assertion there. A 2 s init that nobody measured
is also the kind of thing that turns into a flake on a loaded CI box — the
`test_qemu_rtic_service_e2e` flake seen in the same phase is a reminder of what
that looks like.
