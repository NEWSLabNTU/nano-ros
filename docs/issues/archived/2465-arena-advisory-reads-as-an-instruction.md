---
id: 2465
title: "The default talker's first boot prints the arena advisory as an instruction a first-time user should act on"
status: resolved
type: bug
severity: low
area: [ux, logging, runtime]
related: [phase-523]
found: 2026-11-12
resolved_in: "fix-2465 branch: the advisory names the knob as optional RAM tuning"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md).** Found walking the book's getting-started steps (`just probe bootstrap`, main `80c4b6a5bb`). The 2026-11-07 round-2 industrial adoption probe saw the same line.

## Symptom

The first-node talker, run as the book says, prints at INFO on its first spin:

```
[INFO] nros: arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE=1024 (Zephyr: CONFIG_ prefix). 192/2112 bytes claimed at first spin; later registrations need more.
```

"set X=1024" reads as a step the reader skipped. Nothing is wrong: the arena is larger than this image uses, which costs RAM and nothing else. A first-time user has no reason to act, and "later registrations need more" does not say whether setting it is safe.

## Resolution

The advisory stays at INFO, one-shot, and keeps the value to set, which is what the advisory exists for. Its text now says the change is optional and what it buys:

```
arena larger than needed: 192/2112 bytes claimed at first spin. Optional, to save RAM: NROS_EXECUTOR_ARENA_SIZE=1024 (Zephyr: CONFIG_ prefix), plus room for anything registered later.
```

`executor_arena_advisory` still finds the value to set and checks the line is not truncated. Its substring match moves to the new wording.
