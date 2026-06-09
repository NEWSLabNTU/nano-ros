---
id: 17
title: Zephyr native_sim ↔ zenoh E2E does not connect on some hosts (NSOS offload)
status: open
type: bug
area: zephyr
related: [issue-0018]
---

Surfaced by Phase 225.P (Zephyr workspace Entry). On the maintainer host,
every zephyr-zenoh native_sim E2E fails: the zephyr `zephyr.exe` reports
`Transport(ConnectionFailed)` reaching the host `zenohd`, and the listener
times out with zero messages. This affects the new
`test_zephyr_workspace_entry_native_sim_e2e` **and** the pre-existing
single-node reference `test_zephyr_to_native_e2e` identically.

**Root cause (environmental, not a nano-ros defect)**: the native_sim NSOS
(`CONFIG_NET_SOCKETS_OFFLOAD` + `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS`,
both confirmed `=y` in the built `.config`) host-socket offload is
non-functional in this environment. Evidence: `zenohd` v1.7.2 listens on
`tcp/127.0.0.1:7456` and the host shell connects fine, but when the
native_sim binary runs, (a) `zenohd` logs **no** incoming TCP, (b)
`ss -tnp` shows **no** connection to 7456 during the connect window, and
(c) `strace -f -e connect` on the binary shows **no** `connect()` syscall
to 7456 at all. So NSOS fails the connect *inside* the offload layer
before issuing any host syscall — a Zephyr/native_simulator NSOS-layer
problem (kernel / libc / host-trampoline), independent of nano-ros.

**Impact**: no zephyr-zenoh E2E can pass in this environment. The Phase
225.P workspace Entry itself is correct — it builds via `west build`,
boots, brings up the network, registers its launch node set, and attempts
the session exactly like the proven reference; only the host's NSOS
connectivity blocks delivery.

**To fix / workaround**: run the zephyr E2E in a capable environment (CI,
where the reference test passes), or repair the native_sim NSOS
host-socket offload on this host. Build-only verification (`just zephyr
build-fixtures` with `NROS_ZEPHYR_FIXTURE_FILTER=workspace-entry`) is
green and is the local gate until NSOS connectivity is restored.

**Cross-reference**: the sibling issue #18 (added in the same commit
`5565da2d3`) is now RESOLVED, but via a DIFFERENT path — the cargo-lane
NuttX entry boots on `qemu-system-arm` rather than native_sim — so that
resolution does not transfer to this NSOS host-offload problem.
