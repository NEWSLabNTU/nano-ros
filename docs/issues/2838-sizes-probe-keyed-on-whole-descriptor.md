---
id: 2838
title: "The sizes probe is keyed on the whole sizing descriptor, so every image with its own descriptor builds its own ~165-300 MB probe"
status: open
type: tech-debt
severity: medium
area: [sizing, build, disk, fixtures]
related: [rfc-0100, issue-2702]
found: 2026-11-22
---

**Home:** [RFC-0100](../design/0100-rmw-agnostic-sizing-model.md), D12 amended
2026-11-22 (one configure per image).

## What happens

`nros-sizes-build` measures the C/C++ API's opaque struct sizes by building a
probe copy of the `nros` crates, in a shared directory
`build/sizes-probe/<rustc>/<key>/`. The key (`probe_key`) is the target plus
every forwarded knob. One of those knobs is `NROS_SIZING_DESCRIPTOR`, recorded
as `content:<hash>` of the image's whole descriptor
(`nros-probe-key-inputs.txt`).

Since issue 2702, every C/C++ workspace image has its own configure and its
own descriptor, so every image builds its own probe. Measured on a tier-1
fixture build stopped at the disk floor (2026-11-22):

- `build/sizes-probe/` held 151 probe dirs, 24.8 GB; 86 of them were new in
  that run;
- each probe dir is 165-300 MB.

Before, the images on a coordinate shared one descriptor and so one probe.

## Why it is more than needed

A probe answers "how big is each opaque struct", and only some of the
descriptor's fields change that: the pool counts the structs are sized from.
Others, such as declared QoS rows, the `[meta]` basis and parameter names,
change the hash and not the sizes. So two images whose structs are the same
size still build two probes.

## Fix

Key the probe on the size-relevant fields only: the counts and capacities the
opaque types are parameterised by, read through `nros-sizing-descriptor`, not
the file's bytes. Images that differ only in facts that do not size a struct
then share a probe.

The difficulty is the one phase-336 W7 met: a key that drops a field the sizes
DO depend on hands one image another's sizes, which is worse than not sharing.
So the field set must be derived from what the probe's consumers read, and a
test must build two images that differ in each excluded field and compare
their measured sizes.

Until then the fixture lane prunes the cargo intermediates of each image's
configure (`nros_prune_cargo_intermediates`), and a probe dir keeps all of
its own.
