---
id: 231
title: "Zephyr native IP stack: Cyclone's IP_ADD_MEMBERSHIP join fails (error -1) — firmware runs unicast-only"
status: resolved
resolved_in: "cyclonedds fork 1d794c0a (phase-292 W2)"
type: bug
area: rmw
related: [phase-292, phase-3-asi]
---

## Summary

On Zephyr-native-IP-stack targets (FVP AEMv8R; expected on S32Z too),
every Cyclone multicast group join fails at participant creation:

```
cyclone: error -1 in join conn ... for (udp/239.255.0.1, *) interface udp/192.168.10.2
cyclone: rtps_init: multicast join failed for domain 2 participant -2; continuing unicast-only (Zephyr NSOS)
```

The unicast-only fallback (added for NSOS) engages, so the ASI closed-loop
demo still converges — the firmware's own SPDP multicast TX goes out, the
peer answers unicast — but the firmware cannot RECEIVE multicast SPDP, so
discovery depends on the peer hearing us first, and the host-side tap
needs promiscuous mode.

## Suspects

`ddsrt_setmcast`/`joinleave_asm_mcgroup` issues `setsockopt(IP_ADD_MEMBERSHIP)`
with the Linux-numbered constant injected by `zephyr_ipv4_compat.h`
(`-DIP_ADD_MEMBERSHIP=35`); Zephyr's zsock option numbering differs, and/or
the native stack wants `net_if_ipv4_maddr_add` + IGMP (`CONFIG_NET_IPV4_IGMP=y`
is already in the snippet). The ThreadX twin of this bug was byte-order
(archived: 177.26.RX ntohl mismatch) — check the mreq layout too.

## Fix direction

Map the join onto Zephyr's real option (or call the native maddr/IGMP API
from a zephyr override TU), then delete the promisc requirement from the
ASI demo notes. Verify with a host-side `ros2 topic list` WITHOUT
promiscuous tap.

> **Resolved (2026-07-17).** Root cause: Zephyr's zsock `IP_ADD_MEMBERSHIP`
> handler accepts ONLY `struct ip_mreqn` (strict optlen → EINVAL) and wants
> the interface as `imr_ifindex`; Cyclone passed the classic 8-byte
> `struct ip_mreq`. Fork commit 1d794c0a builds an `ip_mreqn` on
> `__ZEPHYR__` and treats `-EALREADY` (second socket, same iface+group —
> Zephyr membership is per-interface) as success. Verified on the FVP:
> both joins clean, no unicast-only fallback, ASI closed loop at ~19 Hz.
> The compat-header option NUMBER was fine (Zephyr 3.7 uses the Linux
> value natively). Promisc-tap requirement likely obsolete now — drop it
> after a verification run with promisc off (needs root to toggle).
