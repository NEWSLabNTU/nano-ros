---
id: 1363
title: "The NuttX arm C talker publishes nothing at all — its whole transcript is
  one arena advisory reporting 56 of 74240 bytes claimed, and the cell is red in
  every nightly that kept a junit artifact"
status: open
type: bug
area: [testing, rmw, nuttx]
related: [issue-1281, issue-0906, issue-1013, issue-1045, issue-0877, issue-0900]
---

## Symptom

`nros-tests::rtos_e2e test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
fails in the nightly `nuttx` job. The Rust and C++ cells of the same test, on the
same platform, in the same job, pass.

```
nuttx c pubsub E2E failed — heard 0 of the 70 samples this cell requires
(talker printed 0 publish lines) within 160s.
```

Not a single sample, and not a single publish line: this is not the partial
delivery the assertion's own prose describes (a session that expires part-way,
issues 0906/1013). The talker never publishes at all.

## Evidence

Three nightly runs, every one that kept a `nightly-nuttx-junit` artifact:

| date | run | job | this cell | the other two pubsub cells |
| --- | --- | --- | --- | --- |
| 2026-09-11 | 34573245146 | 103180329522 | FAIL | Rust PASS, C++ PASS |
| 2026-09-12 | 34680021029 | 103517111559 | FAIL | Rust PASS, C++ PASS |
| 2026-09-13 | 34744568635 | 103690230027 | FAIL | Rust PASS, C++ PASS |

The failure text is identical across all three.

## The decisive line: the talker's entire transcript

From the 2026-09-13 junit, the harness's own `Talker output:` block holds one
line and nothing else — no banner, no locator, no publish:

```
Talker output:
[INFO] nros: [    0.051000] arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE=1024 (Zephyr: CONFIG_ prefix). 56/74240 bytes claimed at first spin; later registrations need more. issue 0900
```

The C listener in the same cell prints its full banner and claims **11940** of
the same 74240 bytes:

```
Listener output:
nros C Listener
===================
Locator: tcp/10.0.2.2:8300
Domain ID: 0
Support initialized
Node created: listener
Subscriber created for topic: /chatter
Executor created with 1 handle(s)
[INFO] nros: [    0.051000] arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE=12288 … 11940/74240 bytes claimed at first spin …
```

So the talker's executor reached its first spin — 0.051 s of guest time, the same
instant as the listener's — with **56 bytes claimed**, which is an executor with
nothing registered in it. Either the publisher was never created, or the image
never reached the code that creates it. The missing banner does not by itself
decide that: the banner is `printf` on picolibc's buffered stdout while the
advisory is `nros_log`, so an image that stalls before a flush prints exactly
this pair. That is the first thing to resolve by hand.

## What this is NOT

- **Not issue 1281.** That is the RISC-V lane, a different test
  (`c_riscv_nuttx_e2e c_riscv_nuttx_talker_delivers_cross_process`), and a
  different shape: there the guest connects and the router drops the session
  after 28–79 s. Here nothing is ever published and the session's fate is not
  even reached.
- **Not issue 1034.** That was NuttX arm boot latency under the provisioned
  QEMU 11 and is fixed; this cell has 160 s and the guest is demonstrably
  running at 0.051 s.
- **Not the load flakiness the rest of this job shows.** The nuttx action and
  service cells pass in some nightlies and fail in others — all nine cells
  passed on 2026-09-11 except this one. This cell is red in three of three.
- **Not yet excluded: a museum binary.** Both C fixtures were reported FRESH by
  a DEGRADED probe in every one of these runs:
  `INPUT SET UNMEASURED — no build-script record found, compared a hand-authored
  path list instead` (issues 1005/1045), for
  `examples/qemu-armv7a-nuttx/c/{talker,listener}/build-zenoh/*`. A degraded
  probe's green is the absence of a measurement. The listener built from the
  same lane clearly works, which argues against it, but it is not measured.

## Reproduce

Per issue 0877, paste the harness's own command lines verbatim — the emulator is
the provisioned one, not whatever `qemu-system-arm` a shell resolves:

```sh
ZENOH_CONFIG_OVERRIDE='scouting/multicast/enabled=false;listen/endpoints=["tcp/0.0.0.0:8300"]' \
  just zenohd            # resolves the router the way the harness does (issue 0653)
~/.nros/sdk/qemu/*/bin/qemu-system-arm -M virt -cpu cortex-a7 -nographic -icount shift=auto \
  -kernel examples/qemu-armv7a-nuttx/c/talker/build-zenoh/c_talker \
  -netdev user,id=net0 -device virtio-net-device,netdev=net0
```

Run the cell with `NROS_STRICT_STALENESS_PROBE=1` so the degraded probe above
becomes a failure rather than a green.

## What would close it

Either a mechanism for why the C talker's executor claims 56 bytes while the C
listener beside it claims 11940 — which is to say, what stops the publisher from
being created or registered — or a measurement showing the image is stale and a
freshly built one publishes. A fix without one of those two is a guess.
