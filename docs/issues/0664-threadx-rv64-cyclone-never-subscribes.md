---
id: 664
title: "ThreadX-RV64 CycloneDDS images boot, reach the app, and never create a subscriber — three cells, first ever run"
status: open
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

## Worth checking first

* **`Domain ID: 128`.** CLAUDE.md records that Cyclone fixture pairs bake
  DISTINCT domains (50–58) so their SPDP does not collide, and that
  `CONFIG_NROS_CYCLONE_DOMAIN_ID` must default to `NROS_DOMAIN_ID` rather than a
  literal — the phase-180 split-brain. 128 is neither. It may be a legitimate
  default and it may be the bug; it is the first thing to measure.
* **The banner is the zenoh-shaped one** (`Locator: tcp/10.0.2.2:7447`) in a
  CycloneDDS image. Harmless if the example prints a generic banner, worth a
  look if it means the wrong transport config reached the app.
* Whether the subscriber call returns an error that nothing prints — a silent
  early return is exactly what issue 0650's rule is about.

## Why it was never seen

These cells have been skipping on every host that did not hand-build the in-repo
`build/cyclonedds/bin/idlc`, because the provisioned `idlc` was unreachable
(0663). `docs/development/sdk-tiers.md` also still describes them as experimental
behind `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1`, while the lane builds them by
DEFAULT and its doctor says so — doc and code disagree about whether this cell is
supposed to be covered at all. That disagreement should be settled as part of
fixing this: either the cells are supported (and this is a release blocker) or
they are experimental (and the lane should not build them by default).
