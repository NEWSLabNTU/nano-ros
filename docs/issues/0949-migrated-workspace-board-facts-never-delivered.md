---
id: 949
title: "For a migrated workspace, board facts are never delivered — `_ws` resolves to the generated root, which has no `system.toml`"
status: open
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
