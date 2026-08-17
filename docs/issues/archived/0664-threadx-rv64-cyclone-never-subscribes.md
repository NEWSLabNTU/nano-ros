---
id: 664
title: "ThreadX-RV64 CycloneDDS images boot, reach the app, and never create a subscriber — three cells, first ever run"
status: resolved
type: bug
severity: medium
area: rmw/cyclonedds
related: [issue-0663, issue-0650, issue-0085]
---

## Symptom

With `idlc` reachable (issue 0663) the three ThreadX-RV64 Cyclone cells build
and, for the first time, RUN:

```
FAIL test_threadx_riscv64_cyclonedds_two_qemu_pubsub       30.1s
FAIL test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub   30.1s
FAIL test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub  30.3s

threadx riscv64 listener never subscribed: qemu did not print
`Subscriber created for topic:` within the timeout
```

Not a boot failure. The image gets all the way through:

```
[app_define] Creating byte pool… / Running board network init…
[board] Initializing NetX system… / Enabling TCP/UDP/ICMP/IGMP…
[board] BSD sockets initialized
[virtio] init complete / enable: link UP
[app_thread] Calling c_app_main (FFI)…
nros C Listener
Locator: tcp/10.0.2.2:7447
Domain ID: 128
```

…and then nothing. All three languages fail identically, which points at the
board/RMW seam rather than at any one binding.

## Resolved (2026-08-17) — it was aborting, not hanging

A gdb stub on the running image named it in six frames:

```
_exit ← abort ← __emutls_get_address ← thread_states_init
      ← dds_init ← dds_create_domain ← nros_rmw_cyclonedds::session_create
```

CycloneDDS's thread bootstrap reaches libgcc's EMULATED TLS, and
`__emutls_get_address` calls plain `malloc` and `abort()`s when it returns NULL.
The `_sbrk` added in issue 0657 returned failure unconditionally, so it always
did — before any output, which is why it read as a hang.

That refusal was right for what was known when it was written: newlib's malloc
pulls `_sbrk`, picolibc's does not, and allocation on this board belongs to the
ThreadX byte pool, so a libc heap looked like handing out memory that belongs to
something else. What it missed is that `malloc` here has a caller the byte pool
cannot serve, and that caller is emitted by the COMPILER — emutls, not
application code.

**Fix:** `.heap` in `link.lds` reserves 64 KiB (it was deliberately zero-sized)
and `_sbrk` is a bump allocator over it. No free — `_sbrk` has no shape for one,
emutls never frees, and a bump pointer cannot fragment. Sized for emutls's one
small block per TLS variable per thread, not for application data; an app that
wants a heap still uses the byte pool.

**Verified:** the same image prints "Support initialized / Node created:
listener / Subscriber created for topic: /chatter / Waiting for messages", and
all four `threadx_riscv64_qemu` tests pass — C, C++ and Rust Cyclone pubsub —
where three failed on a 30 s timeout. Every riscv64-threadx test in the suite:
5 run, 5 passed.

## Corrections to this issue's own first reading

* **`Domain ID: 128` was a red herring.** It is deliberate: `alloc::domain_of`
  assigns 128/129/127 to the C/C++/Rust pairs so their SPDP stays disjoint.
* **"never creates a subscriber" described the symptom, not the fault.** The
  process was already dead two calls earlier. The banner was the last thing that
  ran, not the last thing that worked — worth remembering when the evidence is
  "output stops here".
* The general lesson: **twelve minutes with a debugger beat two hours of
  reading.** The gdb stub was available the whole time (QEMU `-s`, and the
  toolchain ships `riscv-none-elf-gdb`).

## Still worth settling

`docs/development/sdk-tiers.md` describes these cells as experimental behind
`NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1`, while the lane builds them by DEFAULT
and its doctor says so. Doc and code disagree about whether this platform is
covered; now that the cells pass, the doc is the half that is wrong.
