---
id: 180
title: "examples_canonical_shape red: 12 stm32f4/baremetal rust leaves violate the §212.L.4 class-prefix rule"
status: open
type: tech-debt
area: testing
related: [phase-287, phase-212]
---

## Summary

`examples_canonical_shape::examples_tree_uses_canonical_shape` (the phase-287
W7 lint, `fb4644bde`) fails deterministically: 12 rust leaves under
`examples/stm32f4/rust/*` and `examples/qemu-arm-baremetal/rust/talker-*`
carry `[package.metadata.nros.component] class = "<crate>::<Class>"` whose
prefix is the CRATE name (underscores), while §212.L.4 demands the PACKAGE
name (hyphens):

```
stm32f4/rust/talker: class = "stm32f4_bsp_talker::Talker"
  must start with package name prefix "stm32f4-bsp-talker::"
… (12 leaves total: stm32f4 talker/listener/service-*/action-* × rtic/embassy
   variants + qemu-arm-baremetal talker-rtic/talker-rtic-mixed/talker-xrce)
```

Pre-existing metadata (the leaves predate the lint); surfaced by the first
full `test-all` after the lint landed. Blocks a green `just ci`.

## Fix direction

Either the lint's prefix comparison should normalise `-`/`_` (a Rust class
path CANNOT contain hyphens, so demanding a hyphenated prefix in `class` can
never be satisfied literally — likely the lint needs the sanitized form), or
the 12 leaves' `class` values are wrong in the other direction. Decide which
side is canonical, then sweep.
