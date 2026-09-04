---
id: 1017
title: "Nothing stops a CMake entry template from dropping the session name again
  (issue 1003 has no gate)"
status: resolved
type: tech-debt
area: codegen, api, testing
severity: medium
related: [issue-1003]
found: 2026-09-03
---

## What is unguarded

Issue 1003: all ten `cmake/templates/*_entry_main*_typed.cpp.in` called
`run_components(&__nros_entry_setup)` with no session name, so every image built
through `nano_ros_add_node` registered as `"node"` and XRCE client keys collided.
Fixed by passing the node's name. **No gate was added**, and the acceptance box
for one is still open.

The defect survived from 2026-06-13 to 2026-09-03 in the zephyr template, and
the sibling producer (`emit_cpp.rs`) had been correct since 2026-06-27 — so the
tree spent two months with two spellings of one fact and nothing comparing them.
That is the exact recurrence shape CLAUDE.md's "fix the CLASS, then prove the
sweep" rule exists for.

## Why the obvious gate is wrong

A naive "every `run_components` call in a template passes N arguments" check
fails, because the arity that carries the name differs by board:

| board | overloads |
| --- | --- |
| `LinuxBoard` | `(session_name, setup)`, `(setup)` |
| zephyr / freertos / nuttx / threadx | `(locator, session_name, setup)`, `(locator, setup)`, `(setup)` |

`LinuxBoard` has no locator parameter at all. A uniform 3-arg rule is what broke
the native build while fixing 1003:

```
error: no matching function for call to
  'nros::board::LinuxBoard::run_components(const char [1], const char [9], int32_t (*)())'
```

## Candidate shapes

* **Textual, per board:** the check knows each board's session-name position.
  Cheap and on the fast line, but it is a THIRD authored copy of the overload
  sets — the failure mode this issue is about.
* **Derived from the header:** parse the `run_components` overloads out of
  `main.hpp` and require every template to bind the one that takes a name. One
  spelling, but a C++ signature parser is not free.
* **Behavioural:** assert two same-language images on one agent register
  DISTINCT names. Catches the defect by its consequence rather than its form,
  and would also catch a future producer nobody thought to scan. Needs a
  runtime cell, so it is the most expensive and the most durable.

The behavioural one is the only shape that would have caught the ORIGINAL bug
without anyone knowing the templates existed.

## Acceptance

* [ ] A template that drops the session name fails a check, and the check is in
      a lane something actually runs.
* [ ] The check does not become a fourth hand-maintained copy of the per-board
      overload sets — or if it does, that is a recorded decision with a reason.


## Resolved 2026-09-04 — `check-entry-session-name`, and the trap avoided

`scripts/check-entry-session-name.py`, on the fast line as
`just check entry-session-name`.

**The shape none of the three candidates above quite had.** This issue framed
the choice as textual-per-board / header-derived / behavioural, and warned that
the textual one becomes a third authored copy of the overload sets. It does — if
it checks ARITY. It does not if it checks for a NAME:

    every `run_components(…)` a producer emits must MENTION a session source

That is uniform across all ten templates even though the arity is not
(`LinuxBoard` is `(session_name, setup)`, the RTOS boards
`(locator, session_name, setup)`, and zephyr passes its locator as a macro
rather than a substitution). So the check knows nothing about overloads, and a
future board with a fourth shape satisfies it without this file being touched.

**It covers BOTH producers, which is the point.** #1003 was not one broken
emitter; it was two spellings of one fact with nothing comparing them. The check
asserts the templates carry `NROS_ENTRY_NODE_NAME` and that `emit_cpp.rs` carries
`nros_boot_config_node_name`.

Non-vacuous, verified by mutation on the real code both ways:

* revert one template to `run_components(&__nros_entry_setup)` — the original
  #1003 bug — and it fails, naming the file;
* strip the name from `emit_cpp.rs`'s emission and it fails there instead.

Restored, it reports `OK (13 generated call(s) name a session)`.

**Three ways it refuses to pass while guarding nothing**, because a gate that
silently checks zero things is this campaign's other recurring defect: a glob
matching no files is an error; a NAMED producer file that emits no call is an
error (which is what catches an emitter moved below `#[cfg(test)]`); and zero
calls found overall is an error.

Two things the scanner has to get right, both found by testing rather than
reading: `emit_cpp.rs` splits its call across a Rust string continuation, so
joining lines with a space truncates it at the backslash and reports a false
positive; and that file's own tests assert on the emitted text
(`!src.contains("::run_components(")`), so test code is cut before scanning —
tests are not producers.

## What is still NOT guarded

The behavioural property — that two same-language images on one agent register
DISTINCT names — remains untested. This check proves a name is PASSED, not that
distinct images get distinct names; a producer that passed the same constant
everywhere would satisfy it. That is the shape this issue argued would have
caught the original bug without anyone knowing the templates existed, and it
still needs a runtime cell.
