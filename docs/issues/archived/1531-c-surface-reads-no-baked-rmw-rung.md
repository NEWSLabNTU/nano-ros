---
id: 1531
title: "The C surface reads no baked RMW rung: `NROS_ENTRY_RMW` is on every C
  compile line and has only C++ consumers, so a C image cannot name its backend
  even though `nros_support_init_rmw` takes one"
status: resolved
type: bug
area: [api, rmw]
severity: medium
found: 2026-09-28
related: [1050, 1530, 0196, phase-424]
---

## What is missing

`cmake/NanoRosEntry.cmake:855` bakes `NROS_ENTRY_RMW` onto every entry target,
whatever its language:

```
DEFINES = -DNROS_ENTRY_RMW=\"cyclonedds\" -DNROS_HOST_POSIX -DNROS_PLATFORM_POSIX ...
```

It has exactly two consumers, both C++:
`packages/api/nros-cpp/include/nros/executor.hpp:152` and `node.hpp:2675`.

The C road reads it nowhere. Every in-tree C main calls the three-argument
`nros_support_init`, whose `rmw` is `NULL`, so an image that holds its own
answer in its own preprocessor never passes it. `nros_support_init_rmw` exists
and takes the selector — what is absent is a consumer of the bake.

This is the half of issue 1050 that landed for one language surface only.
`b64e3655a` added the baked rung to cmake and to the C++ headers, and in the same
commit made a selector-less open with more than one registered backend a hard
refusal. The C road got the new failure mode and not the new capability.

## What it cost, and what it still costs

It has already cost one high-severity outage: issue 1530, where a recorder
self-registering into every native C and C++ image made the registry ambiguous
and every native C example failed `nros_support_init` with
`NROS_RET_INVALID_ARGUMENT` for eighteen days. 1530 is fixed by removing the
second registrant, so these images hold exactly one backend again and a nameless
open resolves.

What remains is the capability gap: **a C image that legitimately links two
backends still cannot name one.** No in-tree native C image does today, which is
why this is filed separately rather than folded into 1530 — the symptom is gone
and the asymmetry is not.

## Fix — 2026-09-28

**`<nros/baked_rmw.h>`**, a new hand-written header holding two function-like
macros that forward the bake:

```c
#ifdef NROS_ENTRY_RMW
#define nros_support_init(support, locator, domain_id)                          \
    nros_support_init_rmw((support), (locator), (domain_id), NULL, NROS_ENTRY_RMW)
#define nros_support_init_named(support, locator, domain_id, session_name)      \
    nros_support_init_rmw((support), (locator), (domain_id), (session_name), NROS_ENTRY_RMW)
#endif
```

`nros_support_init_rmw` is deliberately NOT wrapped: a caller naming a backend
has already answered the question the bake answers.

**Placement is load-bearing in two directions**, and both are written at the file:

* it must come AFTER `<nros/nros_generated.h>`, which DECLARES
  `nros_support_init(...)` — a function-like macro of the same name would eat the
  declaration. It is included last from `<nros/types.h>` for that reason;
* it must be reached by EVERY consumer, not only those including
  `<nros/init.h>`. Partial coverage is worse than none: two TUs in one image would
  disagree about whether the bake is passed, and the one that missed it would fail
  exactly the way 1530 failed. `<nros/types.h>` is the single include every module
  header already goes through.

It is NOT in `<nros/entry_config.h>`, which owns the rest of the `NROS_ENTRY_*`
ladder: that header is reached by `app_main.h` / `main.h` BEFORE any declaration
exists, and its documented contract is "preprocessor only — no types, no
includes, no linkage".

## Acceptance — all four met, measured

* **A native C image with two registered backends opens the one its entry baked.**
  Measured by RECREATING the ambiguous image — temporarily restoring the census
  recorder's `.init_array` ctor that issue 1530 removed, so the registry holds
  `cyclonedds` + `metadata` again — and rebuilding `examples/native/c/listener`:

  ```
  $ env -u NROS_RMW ./c_listener        # this exact shape returned -3 before
  Support initialized
  Node created: listener
  ```

* **`$NROS_RMW` still wins over the bake**, and the proof needed a discriminator
  rather than two successful runs: naming a backend the image does NOT register
  must fail if the environment is really winning.

  ```
  $ NROS_RMW=zenoh ./c_listener        # zenoh is not registered in a cyclone image
  nros_support_init(...) -> -3         # env won, resolved to Unknown
  $ env -u NROS_RMW ./c_listener       # bake won
  Support initialized
  ```

  Both directions, on one binary.

* **Taking the address of `nros_support_init` still compiles.** A function-like
  macro expands only when followed by `(`, and
  `packages/api/nros-c/tests/compile/baked_rmw_rung.c` pins it by assigning
  `&nros_support_init` to a typed function pointer. If this were ever written as
  an object-like macro that TU stops compiling.

* **A gate holds the two surfaces to the same rung set.**
  `check-entry-rung-consumers` (fast line) — see below.

## The probe and the gate, and what each one can answer

`packages/api/nros-c/tests/compile/baked_rmw_rung.c` is compiled by `just check c`
**twice, with the bake and without**, and the lane then reads the PREPROCESSOR:
with a bake the three-argument call must become `nros_support_init_rmw` and must
carry the baked value; with no bake the nameless spelling must SURVIVE, because an
image with no bake resolves namelessly as it always has and a macro that fired
anyway would pass a token that does not exist.

Mutation-tested end to end: compiling the two macros out makes `just check c`
exit 1 naming this issue. Two attempts were needed — the first mutation renamed
the macro and died at `c-fmt` on line length before reaching the assertion, which
is a reminder that a mutation has to reach the check it is testing.

The assertions capture first and match through `nros_grep_q` on a here-string.
The first draft piped into `grep -q`, and two gates caught it: `grep -q` cannot
tell a tool error from a non-match (issue 0726), and a status-consuming pipeline
into an early-exiting matcher can report a MATCH as a miss when it SIGPIPEs the
writer (issue 1077). Both failures are load-only, which is the direction that
teaches people to re-run a gate rather than believe it.

**`check-entry-rung-consumers`** is the class rule: a rung that cmake bakes onto
an entry target AND that is a `BootConfig` field must be consumed by EVERY
language surface, or by none. Scope is derived on both sides — the cmake bakes
intersected with `nros_node::BootConfig`'s fields — and never authored, because
plain symmetry over every `NROS_ENTRY_*` fires on `MAX_NODES` / `MAX_ENTITIES`,
which are legitimately C++-only, and an exemption list for those is the "only as
complete as whoever wrote it" problem the gate exists to answer.

Verified against the pre-fix tree, not only its own selftest: with
`baked_rmw.h` removed it FAILS naming `NROS_ENTRY_RMW` and which surface is
missing; with it present, 3 ladder rungs baked, 3 on every surface. The selftest
carries both directions of the asymmetry, so the rule is not written one way only.
