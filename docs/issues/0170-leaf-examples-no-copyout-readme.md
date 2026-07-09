---
id: 170
title: "Leaf examples ship no README — copied-out project carries zero instructions, contradicting the RFC-0026 copy-out contract"
status: open
type: tech-debt
area: docs
related: [rfc-0026, phase-277]
---

## Problem

Examples are framed as **standalone copy-out projects** (RFC-0026, phase-277),
but the canonical leaves ship no `README.md`:

- `examples/native/rust/talker/`, `.../listener/` — the highest-traffic first
  examples — have none.
- Same for every `native/c/*`, `native/cpp/*` (except `custom-platform` and
  `custom-transport-loopback`), `qemu-*/…/{talker,listener}`, and `zephyr/…`
  leaves.
- Run instructions live only in the parent `examples/<platform>/README.md`
  coverage tables — a copied-out `talker/` directory carries nothing.
- `examples/README.md:137` claims "each workspace has its own README"; true for
  `workspaces/*` and `templates/*`, false for the canonical leaves.

Adjacent doc rot in the same file: `examples/README.md:218,260` still describe
the Phase-140 `add_subdirectory` consumption era + `build/zenohd/zenohd`, while
the current C copy-out contract (lines 31-37) is the `-DNANO_ROS_ROOT` guard —
two eras of instructions coexist in one file.

## Fix direction

Template-generate a minimal per-leaf README (3-5 lines: prerequisites /
`source activate.sh`, router start, build+run command, where the copy-out knobs
live). Start with `native/rust/{talker,listener}`. Fold generation into the
example-scaffolding path so new leaves get one automatically. Purge the
Phase-140-era paragraphs from `examples/README.md`.
