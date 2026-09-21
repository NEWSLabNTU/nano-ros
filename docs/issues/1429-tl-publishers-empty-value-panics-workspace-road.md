---
id: 1429
title: "`NROS_DECLARED_TL_PUBLISHERS=\"\"` panics the zpico build script, so tier 1
  cannot build its workspace fixtures — the reader has three arms and the
  workspace road delivers a fourth"
status: open
type: bug
area: [zenoh, tooling, testing]
severity: high
found: 2026-09-21
related: [issue-1378, issue-1015, issue-1033, issue-1341]
---

## What happens

`host-tests` (tier 1) fails on `main` at `Build workspace fixtures`, on two
consecutive heads:

| run | event | head | job |
| --- | --- | --- | --- |
| 35557195007 | schedule 03:20 | `eae3ffdc` | 106203011198 |
| 35558087447 | push 03:36 | `3da6315e` | 106205543517 |

Both die the same way, twice per run (two leaves), inside
`just native build-workspace-fixtures`:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:304:23:
NROS_DECLARED_TL_PUBLISHERS="" is neither a count nor `refused`. It is how many
TRANSIENT_LOCAL publishers the entry declares, each of which opens a cache
queryable (issue 1378).
error: recipe `build-workspace-fixtures` failed on line 259 with exit code 2
```

## Why the value is empty, and why that is a fourth case

The reader (`runner.rs:288-309`, landed with `8be311fb0`, "fix(#1378): an action
server costs FOUR queryables, and two roads of three could not say so") is
deliberately three-valued:

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

The cmake road does not produce it. `cmake/NanoRosEntityFacts.cmake:801-808`
appends `refused` when the global property says unknown, appends the value when
it is non-empty, and **appends nothing otherwise** — the three-valued contract,
correctly. The failing road is the workspace/leaf one:
`packages/cli/nros-cli-core/src/leaf_entity_env.rs` renders the image's
`NROS_DECLARED_*` facts as `[env]` rows and has no `TL_PUBLISHERS` handling at
all (`git grep -n "TL_PUBLISHERS" packages/cli/nros-cli-core/src/leaf_entity_env.rs`
is empty), while it does complete `SERVICE_SERVERS` and `INFRA_QUERYABLES`
explicitly at `:729` and `:737`.

## What this is NOT

- **Not issue 1353.** No `No space left on device`, no `Free space left`, and
  the log is complete rather than truncated mid-compile.
- **Not issue 1345.** That pair is `capability-conditionals` +
  `xrce-vendored-versions` on the hosted push `gate`; this is tier 1 and a
  panicking build script.
- **Not a defect in the refusal.** The panic is the reader doing what 1378 asked
  it to do. Nothing here argues for a silent default — see below.
- **Not confined to one head.** Two heads, two events, four panics.

## What would close it

The fix has **two defensible sites and they say different things**, which is why
this is filed rather than patched:

1. **The reader treats `""` as absent.** Defensible: a variable exported with no
   value carries no claim, and `None` already means exactly that. Cheap, one
   line, and it fixes every road at once. Against it: it converts a malformed
   producer into a silent `0`, which is the shape 1015/1033 warn about — and a
   count of 0 here is a *legitimate* answer, so the reader would stop being able
   to tell "nobody said" from "said nothing".
2. **The workspace road emits a count or `refused`, never empty.** Defensible:
   that is the knob's stated contract, and 1378's whole point was that every
   road must be able to say which of the three it means.
   `leaf_entity_env.rs` already does this for two other facts. Against it: it is
   the narrower fix, and any road added later repeats the mistake.

Doing (2) is what makes the road honest; doing (1) as well is only safe if the
empty case is distinguished from a real `0` — otherwise it re-creates the class.
Whichever is chosen, acceptance is `just native build-workspace-fixtures`
reaching the end on a clean tree, and a gate for the class:
`check-declared-fact-carriers.py` already knows this knob (`:428`) and is the
natural place to assert that a carrier emits one of the three spellings.

Not measured here: whether any *shipped* image is mis-sized by this. The panic
is at build time, so nothing reached a board; what it costs today is tier 1.

## Measured 2026-09-21: no in-tree FACT COMPOSER can emit `""`

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
