---
id: 2883
title: "`epoch_failure = \"halt\"` after a source list ending in `platform` never applies on a port whose `platform` is \"none\", and configure accepts it"
status: open
type: enhancement
severity: low
area: [clock, zephyr, freertos, cli]
related: [phase-478, rfc-0141, 0758]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I43. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

RFC-0141: "`platform` always answers. It is the port's own clock, so as a
fallback it is the image's knowing choice of whatever that port has, 'none'
included; `epoch_failure` applies only when no listed source answered." On
Zephyr, FreeRTOS, ThreadX and NuttX `platform` is "none" (RFC-0141, "Which
source a port can resolve"). So on those ports an image that writes

```toml
[image.<id>.knobs.clock]
source = "sntp,platform"
epoch_failure = "halt"
```

never halts: when SNTP fails, `platform` answers with no clock, stamps read 0,
and the image boots on. The configure accepts the pair in silence, though the
`halt` it states is unreachable by construction.

## Island workaround

None in code; the island learnt the rule and writes around it. Its phase-15
P5: "On Zephyr `platform` always answers, with no clock (stamps read 0), so a
list that ends in it never reaches `halt`: a `halt` image names `sntp`
alone." The four TAP images and `s32z` in
`actuation_module/src/controller_bringup/system.toml` state `source = "sntp"`
with `halt`; the CI images state `sntp,platform` with `unstamped`.

## Wanted

At configure, on a port whose `platform` is "none", a source list that ends in
`platform` together with `epoch_failure = "halt"` (or any value other than
the default) is refused or warned, naming the port, the two rows and the
remedy (drop `platform` from the list, or state `unstamped`). Where `platform`
is a real clock (POSIX, the freertos-posix board), nothing changes.

## Acceptance

- A knob-validation test: Zephyr + `sntp,platform` + `halt` refused (or
  warned, per the owner's choice) with the remedy; POSIX + the same accepted;
  Zephyr + `sntp` + `halt` accepted.
- RFC-0141's `platform` paragraph names the check.
