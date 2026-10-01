---
id: 1554
title: "Three cmake remedies told the reader to write `nano_ros_node_register(... ENTITIES ...)`, which had raised FATAL_ERROR for 24 days — no gate covers a remedy that names a retired spelling"
status: resolved
type: tech-debt
area: [build, tooling]
severity: low
found: 2026-09-29
resolved_in: 2026-10-01
related: [1226, 0196, phase-412, phase-454]
---

## What

phase-412 retired the `ENTITIES` argument of `nano_ros_node_register` on
2026-09-05 and made it raise `FATAL_ERROR`. Three user-facing `message()`
strings kept telling the reader to write it, and were still doing so on
2026-09-29 when the keyword left the grammar:

| site | what it said |
| --- | --- |
| `cmake/NanoRosEntityInventory.cmake` (the broken-declaration `FATAL_ERROR`) | "Fix the `ENTITIES` argument of the named nano_ros_node_register()." |
| `cmake/NanoRosMessageBounds.cmake` (the upper-bound `message(STATUS)`) | "Declare `nano_ros_node_register(... ENTITIES sub:<pkg>/msg/<Name> ...)` to narrow them" |
| `cmake/NanoRosMessageBounds.cmake` (the same advice, written into the generated knobs file as a comment) | "... on every component to narrow them." |

Following any of them lands on the tombstone. The first is the worse one: it is
the remedy attached to a FATAL_ERROR, so it is read exactly when the reader is
already stuck.

All three are fixed in the wave that filed this. **The gap is that nothing
caught them**, and the next retirement will land the same way.

## Why no gate caught it

`scripts/check/check-knob-single-reader.py` does carry an ENTITIES retirement,
and its reach is CORRECT for what it claims: it forbids the PRODUCER
(`_entities_field`, a `"entities":` key in emitted JSON), not the spelling. A
gate that forbade the literal `ENTITIES` in cmake would fire on the tombstone
itself, which is the one place that must name it.

So this is not issue 0196's narrow-reach shape. It is an unchecked class: a
retirement ledger knows the retired spelling and knows what resolves it now, and
nothing joins that to the strings the build prints at a user.

## Fix direction (not decided)

The ledger entry already has `what` and `resolves_now`. Give it one more field —
the single function allowed to name the retired spelling in a user-facing
string, i.e. its tombstone — and check that no other `message(...)` argument in
cmake names it. `#` comments are out of scope (`strip_comments_hash` already
drops them, and prose explaining a removal legitimately names it); the subject
is the STRING ARGUMENTS of `message()`, which is what a reader is shown.

Two things to get right, neither measured yet:

- the exemption must be the tombstone FUNCTION, not the tombstone FILE. The
  existing entry's comment records why: an earlier draft exempted
  `cmake/NanoRosNodeRegister.cmake` wholesale and put a blind spot in the one
  file the producer lived in, so re-adding the real thing there would have
  passed.
- a generated-file comment (the `NanoRosMessageBounds` knobs-file case) is a
  `#` comment in its OUTPUT but a `message`/`string(APPEND)` string argument in
  its SOURCE. It is user-facing and must be in scope, so "skip `#` comments"
  has to mean the cmake file's own comments, not a `#` inside a string literal.

## Acceptance

Planting `ENTITIES` into any `message()` string outside the tombstone function
fails the gate; the tombstone itself, and every `#` comment explaining the
retirement, pass.

## Resolution

The three sites were fixed in the wave that filed this (PR "ENTITIES leaves
the cmake grammar, and the tombstone gets stronger"). The gate landed
afterwards, and it went into `check-retired-cmake-keywords` rather than the
`check-knob-single-reader` ledger the fix direction suggested. That gate
already DISCOVERS every cmake retirement from its tombstone, so the next
retirement arms it with no new ledger row. The ledger knows one retirement;
the class covers all of them.

**What was wrong with that gate, measured:**

- **Its exemption was a directory, not a function.** It skipped `cmake/`
  wholesale because the tombstone lives there, which also hid the three
  remedies. Its reach was wider than its reason: issue 0196's shape, the
  other way round.
- **It had gone blind to `ENTITIES` in this issue's own wave.** That wave
  reworded the refusal to `"${_call}: ENTITIES was retired"` inside the
  shared `_nros_entities_retired()`. The discovery regex needed
  `"<fn>(...): KW was"`, so `ENTITIES` silently dropped out of the
  retired set. The gate's "found nothing" guard never fired because `HOST`
  and `MODEL` were still found. On `origin/main` the gate discovered
  `{HOST, MODEL}` only.
- **Its comment stripper was `line.split("#")`.** That drops a `#` INSIDE a
  string, which is exactly the generated-knobs-file case this issue names.

**Now**, over every tracked cmake file except `tests/` and `third-party/`,
comment-stripped by the shared `scripts/lib/comments.py`:

1. A retirement is found from either wording, and its TOMBSTONE is the
   enclosing `function()`/`macro()`. Any `<KW> was retired|removed` in
   `cmake/` code that the gate cannot attribute is a FAILURE.
2. Some keywords are FULLY retired: no live grammar declares them outside a
   function that refuses them (the tombstone, or a verb handing it ARGN).
   `ENTITIES` and `HOST` are like this. Such a keyword may appear as a token
   nowhere outside its tombstone function. That covers `message()` strings,
   `string(APPEND)` text written into a generated file, revived grammars and
   callers. `#` comments are free.
3. Other keywords are PARTIALLY retired: retired from one verb and live in
   others (`MODEL`). Such a keyword is refused when a caller outside
   `cmake/` passes it (the 1033 rule, unchanged), or when a call into the
   retiring verb passes it from anywhere.
4. A tombstone's remedy may name only keywords its verb still accepts. For a
   shared tombstone, that means the verbs that call it. A name counts as
   such only in keyword position: inside backticks, or followed by a value.
   This repo's prose capitalises for emphasis ("ONCE PER SYSTEM"), and
   `SYSTEM` is some other verb's keyword.

**Rule 4 found a live instance on its first run.** `nano_ros_entry`'s `HOST`
tombstone still told the reader to "point MODEL at the per-host SystemModel
… e.g. `MODEL config/multihost_<h>_model.yaml`". `nano_ros_entry` has not
parsed `MODEL` since phase-330 W4.a / phase-405 W4. The sibling tombstone in
`nano_ros_add_executable` had been corrected, and this one had not. It now
gives the same live answer (`BRINGUP <dir> LAUNCH <multihost.launch.xml>
LAUNCH_ARGS host=<h>`).

**Acceptance, measured:**

- The self-test runs on the normal path, with 18 synthetic cases. These
  pass when they should:
  - the tombstones themselves;
  - `#` comments explaining the retirement;
  - `MODEL` passed to a verb where it is still live;
  - prose emphasis that spells another verb's keyword;
  - a shared tombstone naming its calling verb's keyword;
  - `MAX_ENTITIES` as a substring.

  These fail when they should:
  - `ENTITIES` planted in a `message()` remedy;
  - `ENTITIES` planted in a `#`-led string written into a generated file;
  - a revived grammar;
  - a caller passing `ENTITIES`, bare or with values;
  - `HOST`;
  - `MODEL` passed to the verb that retired it, from a caller or from the
    API;
  - a remedy naming a dead keyword, in prose position or in backticks;
  - a retirement reworded so it cannot be attributed;
  - no tombstone at all.
- **Replay.** The tree just before that wave (`cmake/` + `examples/zephyr`
  extracted from the parent of the "ENTITIES leaves the cmake grammar"
  commit) is GREEN under the old gate. The new gate flags 8 lines there:
  - all three remedies this issue lists: the FATAL_ERROR at
    NanoRosEntityInventory.cmake:553, the `message(STATUS)` at
    NanoRosMessageBounds.cmake:744, and the generated-file comment at
    NanoRosMessageBounds.cmake:1218/1226, a `#` inside a string;
  - `nros_components_register_node`'s grammar still parsing and forwarding
    `ENTITIES` (NanoRosVerbs.cmake:479/554/555);
  - the `HOST`→`MODEL` remedy above.
- The current tree passes, with the retired set
  `{ENTITIES (_nros_entities_retired), HOST (nano_ros_add_executable,
  nano_ros_entry), MODEL (nano_ros_add_executable)}`.

**Out of the gate's scope, swept by hand:** prose that presents `ENTITIES` as
the live declaration surface. `.env.example`,
`book/src/reference/environment-variables.md` and
`docs/guides/esp32-setup.md` now name the contract sidecar, or a standalone
leaf's `system.toml` `[[component]] entities`. Explanatory mentions in
RFCs, changelogs and `system.toml` comments ("the grammar the retired …
used") are history, not remedies, and are left alone. A remedy in prose is not
statically decidable from an explanation of a removal. That is the same
reason `#` comments are exempt in cmake.
