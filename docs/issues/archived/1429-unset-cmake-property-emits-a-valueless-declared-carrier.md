---
id: 1429
title: "`NROS_DECLARED_TL_PUBLISHERS=\"\"` panics the zpico build script — an
  UNSET cmake property emits a valueless carrier, because `if(NOT _v STREQUAL
  \"\")` compares the variable's NAME"
status: resolved
type: bug
area: [zenoh, tooling, testing, build]
severity: high
found: 2026-09-21
resolved: 2026-09-21
related: [issue-1378, issue-1015, issue-1033, issue-1341, issue-1226, issue-0319]
---

## What happens

Two lanes, one cause.

**Tier 1.** `host-tests` fails on `main` at `Build workspace fixtures`, on two
consecutive heads:

| run | event | head | job |
| --- | --- | --- | --- |
| 35557195007 | schedule 03:20 | `eae3ffdc` | 106203011198 |
| 35558087447 | push 03:36 | `3da6315e` | 106205543517 |

**`check-template-copy-out`.** Reproduced on a clean `origin/main` worktree
(`56db6302c`), CLI and launch-resolver freshly built, gate run solo:

```
  c-and-cpp-mixed-workspace: FAIL — the copy does not build
  multi-node-workspace-cpp:  FAIL — the copy does not build
  pure-c-workspace:          FAIL — the copy does not build
check-template-copy-out: FAILED — a template a user copies does not build.
```

Three of the six buildable templates — **every template that builds a C or C++
workspace**, i.e. the shape a user copies out. Each dies the same way, inside
`_cargo-build_nros_c` / `_cargo-build_nros_cpp`:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:304:23:
NROS_DECLARED_TL_PUBLISHERS="" is neither a count nor `refused`. It is how many
TRANSIENT_LOCAL publishers the entry declares, each of which opens a cache
queryable (issue 1378).
```

## Why the value is empty, and why that is a fourth case

The reader (`runner.rs`, landed with `8be311fb0` for issue 1378) is deliberately
three-valued:

| value | meaning | arm |
| --- | --- | --- |
| absent (`None`) | this road states nothing | `0` |
| `refused` | the road tried and cannot bound it | `0` + a `cargo:warning` |
| a count | the bound | the count |
| **`""`** | — | **falls into the count arm, fails to parse, panics** |

The refusal is the point: issues 1015 and 1033 are the same family, where a
silent default over a derived count either starved a pool or wasted 33 KB, so
"neither a count nor `refused`" is written to be loud rather than assumed. An
empty string is the one input that carries no claim at all and still reaches the
arm that demands one.

## The cause — MEASURED, and not where this issue first looked

The first filing said the cmake road was correct and blamed the workspace/leaf
road (`leaf_entity_env.rs`, "no `TL_PUBLISHERS` handling at all"). **Both halves
of that are wrong.** `leaf_entity_env::leaf_facts` starts from
`entity_facts::facts_from_model`, which OMITS the key when it abstains and
inserts a non-empty string otherwise; `facts_from_leaf` routes every answer
through `tl_token`, which is `refused` or a count. **No Rust producer in the tree
can compose a valueless carrier.** The cmake road is the only one that can, and
it did.

The emitter reads like the three-valued contract:

```cmake
get_property(_tl_unknown GLOBAL PROPERTY NROS_ENTITY_TL_PUBLISHERS_UNKNOWN)
get_property(_tl         GLOBAL PROPERTY NROS_ENTITY_TL_PUBLISHERS_MAX)
if(_tl_unknown)
    list(APPEND _env "NROS_DECLARED_TL_PUBLISHERS=refused")
elseif(NOT _tl STREQUAL "")            # <- never fires
    list(APPEND _env "NROS_DECLARED_TL_PUBLISHERS=${_tl}")
```

**`get_property()` leaves its output variable UNDEFINED when the property was
never set — it does not set it to the empty string.** `if()` dereferences a bare
word only when it names a DEFINED variable, and otherwise compares the LITERAL
NAME. So for an unset property the guard asks `"_tl" STREQUAL ""`, which is never
equal, the `elseif` is TAKEN, and the emit line interpolates an undefined
variable to nothing:

```
cmake -E env ... NROS_DECLARED_NODES=2 NROS_DECLARED_TL_PUBLISHERS= cargo rustc ...
```

Measured on cmake 3.22 with a standalone probe, and the two cases really do
differ:

| property state | `_tl` | `NOT _tl STREQUAL ""` | result |
| --- | --- | --- | --- |
| **never set** | UNDEFINED | **TRUE** | **emits `NAME=`** |
| set to `""` | defined, empty | FALSE | skipped, correctly |
| set to `"0"` | `0` | TRUE | emits `NAME=0`, correctly |

The configure says so itself, and nobody read it — the status line carries the
same interpolation:

```
-- nano-ros: queryable table sized from the declaration — infrastructure none,
   application count undeclared ...,  transient-local publisher(s)
                                     ^ the count that was not there
```

Which property is unset is decided by the MODEL: `facts_from_model` emits
`TL_PUBLISHERS` only when the model `describes_wiring`, and emits no `refused`
word at all, so an abstaining model sets neither `..._MAX` nor `..._UNKNOWN`.
109 of the tree's 114 resolvable models are that shape.

### The class: three sites, two of them masked

```
git grep -n 'get_property' -A3 -- 'cmake/**' | grep 'STREQUAL ""'
```

finds exactly three, all in `nros_entity_facts_env`, all carrying the same
idiom:

| site | carrier | live? |
| --- | --- | --- |
| `NROS_ENTITY_NODES_MAX` | `NROS_DECLARED_NODES` | masked — every road emits a node count, so the property is always set |
| `NROS_ENTITY_SERVERS_MAX` | `NROS_DECLARED_SERVICE_SERVERS` | masked — an abstaining road sets `..._UNKNOWN`, and the `AND` short-circuits |
| `NROS_ENTITY_TL_PUBLISHERS_MAX` | `NROS_DECLARED_TL_PUBLISHERS` | **LIVE** — 1378 gave it a `refused` word but no road emits one, so abstention leaves both properties unset |

Two of the three are correct by accident, not by construction. This is the
Zephyr unset-variable class CLAUDE.md already records (#282 fixed 1 of 6 sites
and added a second idiom instead of a shared helper -> #326).

And the READER had the same shape: three of its rules panic on an empty carrier
(`queryable_default_from` on `SERVICE_SERVERS`, `declared_nodes` on `NODES`,
`declared_transient_local_publishers_from` on `TL_PUBLISHERS`) and one silently
mis-reads it (`queryable_floor_from`'s `is_none()` test treats `Some("")` as a
declaration). The third was found by the regression test, not by reading.

## Why it stayed red

`check-template-copy-out` runs on `schedule` / `workflow_dispatch` only. No
merge-gating event runs it, so nothing between the commit and its merge asked.
That is issue 1226's shape ("a gate that works is not a gate that runs") and
issue 0319's before it.

## Resolution

**Both sides, with the ownership stated.**

1. **The emitter owns the defect.** `_nros_entity_fact(<out> <PROPERTY>)` in
   `cmake/NanoRosEntityFacts.cmake` reads an accumulator property into a
   variable that is ALWAYS DEFINED; all three sites go through it. A quoted
   `if(NOT "${_tl}" STREQUAL "")` is equally correct and was rejected: it is
   invisible, so nothing distinguishes a site that was fixed from one nobody
   looked at, and it is a second idiom beside the first — #326's mistake.

2. **The reader is TOTAL anyway**, because a build script does not own its
   environment. A carrier can arrive empty from a CI `env:` block, a
   `cmake -E env VAR=`, or a wrapper that exports every name it knows, none of
   which the emitter can reach; a library build script that aborts on someone
   else's blank variable is hostile, and it aborts where there is no context to
   explain itself. One predicate, `stated()`, at the boundary of every rule,
   plus `declared_fact()` as the single env reader.

   **Total is not silent.** An empty carrier emits a `cargo:warning` naming the
   variable and this issue, so a producer regression is still audible — it
   costs a warning instead of the build. Reading empty as ABSENT is safe here
   precisely because `0` keeps its own spelling: the two are not conflated, and
   the 1015/1033 "silent default over a derived count" shape does not apply to
   a value that states nothing.

3. **Neither side can reintroduce it silently.** `check-declared-fact-carriers`
   gains **rule 4 (VALUED)**: a variable filled by `get_property()` may not be
   compared against `""` unquoted. It is checked as the IDIOM rather than as a
   name, because the emit line is one variable away from the guard and any of
   them can be the next carrier. Five self-test cases, including the two-
   character difference between the broken and the correct spelling, and the
   `set()` case that must stay clean. That gate is on the **fast lane**, so this
   class is now merge-gated at no build cost — which `check-template-copy-out`
   itself cannot be (see below).

### Measured

| | before | after |
| --- | --- | --- |
| `just check template-copy-out` | 3 of 6 FAIL | **6 of 6 OK** |
| `check-declared-fact-carriers` | 26 facts, 0 problems | 26 facts, 0 problems, rule 4 green |

Regression test:
`runner::queryable_default_tests::an_empty_declared_carrier_states_nothing_and_is_not_a_build_failure`.
Proved to fail without the fix by neutering `stated()` to the identity — it
reproduces the production panic text verbatim — and the gate's rule 4 proved to
fire by restoring one `get_property` site.

### Not fixed here: the gate's reachability

`check-template-copy-out` builds six copy-out projects end to end (~20 min warm)
and needs a provisioned CLI, the launch resolver and vendored sources, so it
cannot join an affordability tier — a gate there may only resolve artifacts the
job itself builds (`check-lane-contracts`). Making it merge-gating is the wrong
answer to this bug; making its CLASS merge-gating, which rule 4 does, is the
right one. Whether the template lane should additionally move from `schedule` to
`merge_group` on its own cost argument is a separate decision and is left open.

## The measurement that pointed here, carried from the open issue

Main's copy of this issue gained the section below on 2026-09-21 (PR #1145),
before the fix landed. It is kept because it is the reasoning that located the
carrier: it cleared all three FACT COMPOSERS by reading them, and concluded that
whatever turned an absent fact into a present empty one had to be downstream of
them. That is exactly what the fix above found — an UNSET cmake property
expanding inside the carrier — so the section is a record of how the search
narrowed, not a competing diagnosis.

### Measured 2026-09-21: no in-tree FACT COMPOSER can emit `""`

Still red, on a third head (`9cecc9b56`, `host-tests` run 35562951985, job
106219153029, step `Build workspace fixtures`) and, since this issue was
written, on a second lane: `live-peer regression` run 35560491407, job
106212324809 (`rows whose board IS this runner`), same frame, same message.

Reading the producers on `main` narrows where the empty string can come from,
and it is none of the three places the sections above look at:

* `packages/cli/nros-cli-core/src/cmd/entity_facts.rs:368` — `tl_token` maps
  `Fact::Stated(n)` to the number and BOTH `Refused` and `Absent` to the word
  `refused`. There is no arm that yields `""`. The leaf road's other insertion
  (`:143`) is behind `if let Some(n) = declared_action_servers(model)`, so when
  the model does not describe wiring the key is simply **absent**, which the
  reader already handles as `None => 0`.
* `cmake/NanoRosEntityFacts.cmake:801-808` — emits `…=refused` when
  `NROS_ENTITY_TL_PUBLISHERS_UNKNOWN` is set, `…=${_tl}` only under
  `elseif(NOT _tl STREQUAL "")`, and otherwise appends nothing. The empty case
  is explicitly excluded.
* `packages/cli/nros-cli-core/src/cmd/build.rs:2097` — `derived_pool_env`, the
  workspace-image road, calls `render_env_sidecar` (not the `_with_facts`
  variant), so it passes an EMPTY facts map and contributes no
  `NROS_DECLARED_*` row at all.

So the malformed value is introduced by a **carrier**, not by a composer: an
`[env]` row or an `env`-prefixed command line expanding a variable that is
unset, which renders as `KEY=` rather than as the key being omitted. That
changes what fix (2) above means — it is not "teach the workspace road to spell
the three answers", which the composers already do, but "stop a carrier from
turning an ABSENT fact into a PRESENT empty one". Fix (1)'s objection is
weaker against that reading: an empty carrier value is not a producer claiming
`0`, it is a carrier that failed to omit an absent key.

What this does NOT establish: which carrier. The three composers above are the
only `NROS_DECLARED_TL_PUBLISHERS` writers a grep of `main` finds outside
`docs/`, so the carrier is assembling the variable from a value it did not get
from them — reproducing it needs the failing fixture's actual command line,
which the CI log does not print. That is the next measurement, and it is why
this is still filed rather than patched.
