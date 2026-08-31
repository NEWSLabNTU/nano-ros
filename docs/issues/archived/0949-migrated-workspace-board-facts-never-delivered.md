---
id: 949
title: "For a migrated workspace, board facts are never delivered — `_ws` resolves to the generated root, which has no `system.toml`"
status: resolved
area: build
severity: medium
found: 2026-08-31
related: [0941, 0950, phase-405, RFC-0065, RFC-0072]
---

# The resolver is pointed at a directory that cannot answer

`nros_resolve_board_facts` reads `system.toml` from the workspace directory it
is given. For a workspace migrated to RFC-0065 D8, `_ws` resolves to the
**generated root** — `build/<coord>/`, written by `nros build` — and that
directory carries no `system.toml`.

So the lookup cannot succeed, on any path, for any board. Not "resolves the
wrong deploy": resolves nothing, for embedded images too.

## Why nobody noticed

Issue 0941's soft-failure behaviour. A configure that cannot resolve board facts
printed a STATUS line and continued, so the image built without `NROS_BOARD` /
`NROS_BOARD_TOML` / `NROS_NETSTACK` rather than failing. 0941 makes the
*classification* explicit — this path now reports a named reason instead of a
shrug — but the underlying miss is a separate bug, and 0941 deliberately did not
change which directory gets read.

## Work

Decide what `_ws` should mean for a migrated workspace and make the resolver ask
the directory that actually holds the bringup, not the generated artifact root.
Then verify by CONFIGURING a migrated workspace and asserting the three
variables are set — `just check fast` never runs cmake, so a grep proves nothing
here.

Found while implementing 0941; not fixed there because the fix changes which
directory every migrated workspace reads, which is its own blast radius.

## Resolved — 2026-08-31, re-derived against #0951

The fix is one `set()`, and it is smaller than the original attempt because
#0951's sibling work had already built the plumbing and left the last link out.

`nros_resolve_board_facts` looks for the workspace as
`NROS_WORKSPACE_DIR -> APPLICATION_SOURCE_DIR -> CMAKE_SOURCE_DIR`, and
**nothing set the first one**. Meanwhile `nano_ros_workspace` already receives
`WORKSPACE_ROOT` from the generated root and resolves it carefully — then kept
it to itself. `nano_ros_workspace` now publishes it (`CACHE INTERNAL`, since
board-facts resolves in another scope).

#0951 did NOT touch this. It changed which KEY holds site config; 0949 is about
which FILE is read at all. Orthogonal — which is why the defect survived a
redesign of the same area, and why re-deriving beat rebasing the old fix.

**MEASURED** on `examples/workspaces/mixed`, `demo_bringup:freertos`:

```
BEFORE  board facts NOT delivered from .../mixed/build/freertos-zenoh-mps2-an385-freertos
AFTER   board facts from .../examples/workspaces/mixed — 5 value(s) delivered to cargo
```

The first attempt at that measurement was INVALID and announced itself by
producing identical lines: `NROS_WORKSPACE_DIR` is `CACHE INTERNAL`, so the
"before" configure read what the fixed one had cached. Clearing
`NROS_BOARD_FACTS_ENV*` alone does not clear it.
