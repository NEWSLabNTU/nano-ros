---
id: 1619
title: "threadx-riscv64 Rust images dial the board's DEFAULT locator
  (`tcp/192.0.3.1:7447`) from a NIC configured by `NROS_APP_CONFIG`
  (`10.0.2.40`/gw `10.0.2.2`) — two identities in one image, so no session ever
  forms under the slirp the test launcher uses"
status: open
type: bug
area: boards, threadx
severity: medium
found: 2026-10-01
related: [1355, 1557, 0181]
---

## What was measured

`rv-virt-threadx/rust/talker`, zenoh, built through the lane's own
`build_threadx_cmake_rmw` path (the only path these six leaves have since
phase-369 W2), run under `qemu-system-riscv64 … -netdev user` (what
`QemuProcess::start_riscv64_virt` launches), with a QEMU `filter-dump` on the
NIC:

```
ARP, Request who-has 10.0.2.2 tell 10.0.2.40
ARP, Reply 10.0.2.2 is-at 52:55:0a:00:02:02
IP 10.0.2.40.58840 > 192.0.3.1.7447: Flags [S]   (repeated every 1 s, never answered)
```

while the console prints `[app] MAC 52:54:00:12:34:56  IP 192.0.3.10  domain 0`.

Three different network identities are in play:

| source | IP | gateway | locator |
| --- | --- | --- | --- |
| NetX on the wire — `NROS_APP_CONFIG` (`threadx_qemu_riscv64_build.rs::emit_nros_app_config`, applied by `startup.c`) | 10.0.2.40 | 10.0.2.2 | `tcp/10.0.2.2:7553` (unused) |
| what zenoh dials — `Config::default()` (`nros-board-threadx-qemu-riscv64/src/config.rs`) | 192.0.3.10 (printed only) | 192.0.3.1 | `tcp/192.0.3.1:7447` |
| what the leaf declares — `examples/rv-virt-threadx/rust/talker/system.toml` `[image.rv-virt-threadx]` | 10.0.2.15 | 10.0.2.2 | `tcp/10.0.2.2:9400` |

`app_main!` → `run_app_thread(Config::default(), None, …)` — no deploy overlay
on this path, so the `[image.*]` block is inert, and `config.rs`'s comment
("The zenoh path is unaffected: its deploy overlay overrides after `default()`")
describes the retired cargo path. Slirp forwards `192.0.3.1` to the real
network (TEST-NET-1), so the SYN is never answered and the image idles.

## Proof that this is the whole blocker

With ONLY `Config::default()`'s locator changed to `tcp/10.0.2.2:7553`
(measurement build, not committed) and `rmw_zenohd` on host `127.0.0.1:7553`:
the talker reaches `Application setup complete`, publishes, and a concurrently
run listener prints `I heard: [Hello World: 1]` … — 52 samples in one run, 49 in
another. So NetX, virtio-net and zenoh-pico on this board work; issue 1355's
"no zenoh session reached the router in 60 s" was this.

## Shape of the fix

One identity per image, from one source: the Rust cmake path should take its
locator (and IP plan) from the same `NROS_APP_CONFIG` the C startup already
applies to NetX — or the leaf's `[image.*]` block should reach both. Whatever
the source, `system.toml`, `NROS_APP_CONFIG` and the test launcher's slirp plan
must agree, and the router port the e2e harness starts
(`platform::THREADX_RISCV.zenohd_port`) must be the one baked.

Also seen, not investigated: a single `[TX] TIMEOUT: no completion` from
`virtio_net_nx.c` early in every run (the busy-wait is a fixed 100,000 spins);
delivery proceeds regardless.
