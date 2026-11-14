---
id: 2531
title: "The store-resolved install RFC-0095 aims at is not built: `nros build` cannot fetch the nano-ros version a project pins"
status: open
type: feature
severity: medium
area: [setup, release]
related: [rfc-0095, rfc-0040]
found: 2026-11-14
---

**Moved here from RFC-0095's "end state"** when the RFC became Stable
(phase-555 W3, rule A1(a)). RFC-0095 states the goal and says it "does not
implement that": it removes the assumptions that made it unreachable (phase-440,
complete).

The goal, from the RFC:

```
curl -fsSL <installer> | sh        # puts `nros` on PATH, nothing else
cd ~/my-robot
nros build
```

`nros` resolves the nano-ros version the project pins and builds against it,
the way `cargo` uses a `rust-toolchain.toml` without the user siting the Rust
source tree.

What is missing is the step that fetches and selects that version from the
store. RFC-0040 stays Draft until this exists (its D3 still describes the
contributor's checkout road); when it does, superseding RFC-0040 by RFC-0095 is
to be revisited.
