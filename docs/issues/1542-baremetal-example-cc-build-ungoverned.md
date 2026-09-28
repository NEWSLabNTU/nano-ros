---
id: 1542
title: "An ungoverned `cc::Build` in the baremetal C talker example, on the compiler issue 0478 is about"
status: open
type: bug
area: [build, examples]
severity: low
found: 2026-09-28
related: [phase-472, 0478]
---

## What happens

`examples/mps2-an385-baremetal/c/talker/build.rs:94`:

```rust
let mut build = cc::Build::new();
```

builds C with `arm-none-eabi-gcc` outside `nros-cc-flags`, the governed wrapper
every `packages/**` build script goes through. VERIFIED present on `main`.

`check-cc-build-policy` reads `packages/**/*.rs` only, so a copy-out example is
outside its population; the same file copied under `packages/` fails the gate.

## What is NOT established

Whether copy-out examples SHOULD take `nros-cc-flags`. RFC-0026 makes examples
standalone, and a governed dependency may be the wrong shape for them. The answer
is either "adopt the wrapper" or "an exemption with a reason" — not silence.

## Fix

Decide, then either route it through `nros-cc-flags` or give examples a stated
exemption; widen the gate either way (phase-472 W5).
