---
id: 2794
title: "An owned entry with several programs is sized from one program's census: the first `nano_ros_add_executable`, so the others' entities are never counted"
status: open
type: bug
severity: high
area: [sizing, census, owned-entry, cmake]
related: [rfc-0100, rfc-0071, issue-2433]
found: 2026-11-22
---

**Home:** [phase-583](../roadmap/phase-583-owned-entry-executables.md) W4,
which builds several programs per owned entry (RFC-0071 D20).

## What happens

On a cross board, an owned entry's pools are sized by its census (issue 2433):
`nros build` builds the entry's package for the host, runs the program, and
records the entities it creates. The census runs ONE program. Which one is
decided by `entity_census.rs`:

- `named = leaf_executable_name(pkg)`: the FIRST `nano_ros_add_executable(<name>`
  in the package's `CMakeLists.txt`. When it matches, that program is the
  census and nothing else is looked at.
- Only when no such call is found does `census_program_binary` read the host
  build's `nros/census-program.txt`. That one refuses with two executables
  unless a `[[component]] name` picks one.

So a package whose programs are all `nano_ros_add_executable` calls is sized
from the first one, silently. Every program links the same runtime
staticlib, whose pool sizes are compile-time constants, so the other programs
run with the first program's pools.

## Direction

An **UNDER-size**, whenever a program after the first creates more entities of
some kind than the first does: more publishers, subscriptions, services,
clients, action servers or clients, timers, nodes, parameters, or a parameter
store where the first has none. That program boots onto pools sized for
another program, and fails at entity creation (a full table, a refused
queryable slot) or, where a pool has no checked exhaustion path, at run time.

Not yet measured on hardware: the code path is read, not reproduced. A
reproduction is a cross-board owned entry with two `nano_ros_add_executable`
programs, the second declaring one more service than the first, and the
second program run on the board.

## Fix

RFC-0100 D12 already answers several entries sharing one runtime: the
runtime's own descriptor, `nros/sizing/runtime/shared.toml`, folded per knob
by max from each entry's. Several programs of one entry take the same rule
(agreed with the sizing descriptor's owner, 2026-11-22):

- each program's census writes `<build>/nros/sizing/<entry>/<exe>.toml`
  (path computed only in `nros_sizing_descriptor`);
- those are the fold's inputs, and cargo is named the runtime descriptor;
- one program keeps today's `<entry>.toml`, byte for byte.

Tests the fix owes: the parameter store folds so that a program with a store
and one without gives a store, never none (the store's carve assertion would
refuse the shared build otherwise), and D12's refusal for disagreeing
per-endpoint QoS fires between two programs as it does between two entries.
