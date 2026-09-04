---
id: 1017
title: "Nothing stops a CMake entry template from dropping the session name again
  (issue 1003 has no gate)"
status: open
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
