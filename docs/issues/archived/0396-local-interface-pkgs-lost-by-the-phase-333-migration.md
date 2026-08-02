---
id: 0396
title: features + local-msg-package cannot sync — the phase-333 migration
  drops workspace-LOCAL interface crates from the patch table
status: resolved
resolved_in: 0a94e9946 + d460c1a43
severity: high
created: 2026-08-03
tags: [codegen, sync, phase-333-fallout]
related: [0367, 0368, 0378]
phases: [phase-333]
rfcs: [RFC-0067]
---

## Symptom

On current main, `nros sync` hard-fails in two workspaces:

```
examples/workspaces/features:
  sync: refusing to write .cargo/config.toml — it would DROP 1
  still-declared generated interface crate(s): custom_msgs.
examples/templates/local-msg-package (src/rust_consumer):
  … would DROP 2 still-declared generated interface crate(s):
  extra_msgs, local_msgs.
```

That refusal is the phase-327 narrowing guard doing its job (the 0363-class
protection): the NEW patch table sync wants to write no longer carries the
entries for the workspace-LOCAL interface packages, and the guard refuses to
silently orphan crates the workspace still declares (`custom_msgs = "*"`,
`local_msgs = "*"`, `extra_msgs = "*"` in member manifests).

Verified pre-existing: both workspaces fail identically with the
member-spelling sweep (`e487a532b`) reverted — this is the phase-333
migration itself, not the follow-up fix.

## Why

Phase-333 W1 moved STANDARD message deps from registry-name + patch-table
to direct path deps, and the sync-side patch-table generation was narrowed
accordingly. Workspace-local interface packages (in-workspace `package.xml`
msg pkgs: `custom_msgs`, `local_msgs`, `extra_msgs`) still use the OLD
mechanism — registry-name dep + sync-managed `[patch.crates-io]` — but the
narrowed generation no longer emits their entries (either their codegen no
longer runs in the sync flow, or the entry-writing set now excludes them).
Result: the two workspaces that exercise local interface packages cannot
sync at all, and anything walking them (generate-bindings' metadata
harness, their fixture rows) fails downstream.

## Fix directions

- **A (consistent with RFC-0067):** migrate local interface pkgs to the
  same path-dep shape — the member declares
  `custom_msgs = { path = "../../generated/custom_msgs", version = "0.0.0" }`
  and the patch entries disappear legitimately. Then teach the narrowing
  guard that a crate declared as a PATH dep is not "dropped".
- **B:** keep local pkgs on the patch mechanism and restore their entries
  in sync's table generation (the pre-333 behavior, scoped to local pkgs).

A is where RFC-0067 is clearly heading; B is the smaller stopgap. Either
way, add the two workspaces' `nros sync` to whatever gate covers the
migration (they are the ONLY in-tree exercisers of local interface pkgs —
exactly why the migration missed them).
