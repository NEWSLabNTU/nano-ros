---
id: 625
title: "A provisioned tool is found by SCANNING the shared store, not by reading the project's pin — so resolution is inconsistent and cannot serve two projects"
status: open
type: design
severity: high
area: build, provisioning
related: [issue-0493, issue-0500, issue-0616, rfc-0014, phase-354]
---

## The two problems, stated

1. **Resolution is not consistent.** One tool is found by three different
   routes in one configure, and they can disagree about the version.
2. **It cannot serve two projects.** A user with an older checkout and a newer
   one has one shared store and no per-project answer. This is general to every
   provisioned tool, not specific to Corrosion.

Corrosion is only where it was caught. One `lane=all` configure on this host
produced **155 resolutions of 0.5.1 against 28 of 0.6.1** — in a tree whose
index pins `0.6.1-nros1`.

## What exists today

The pin is explicit and per-project, in the repo:

```toml
[tool.corrosion]
version  = "0.6.1-nros1"
upstream = "v0.6.1"
```

The store is already keyed by version, for every tool:

```
~/.nros/sdk/corrosion/0.6.1-nros1/
~/.nros/sdk/corrosion/0.5.1-nros1/
~/.nros/sdk/corrosion/{lib,share}/      <- legacy UNVERSIONED install
~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/
```

So both halves of a correct design are already in place. What is missing is the
join: **nothing resolves the pin against the store.**

| route | how it picks | reads the pin? |
| --- | --- | --- |
| already-loaded (parent scope) | whatever a parent `find_package` left | no |
| SDK store | `file(GLOB)` + `COMPARE NATURAL ORDER DESCENDING`, newest first | **no** |
| FetchContent fallback | `[tool.corrosion] upstream` | yes (tag only) |

`_nros_corrosion_pin()` does read the index — but only for the FetchContent
TAG. The store lookup right beside it globs and sorts. The newest-first ordering
is a HEURISTIC STANDING IN FOR THE PIN, which is why issue 0500 had to exist at
all, and why it needs a gate (`check-cmake-corrosion-prefix`) to keep two
implementations of the sort agreeing.

## Why the heuristic cannot be made right

"Newest in the store" is a global answer to a per-project question.

* It is wrong whenever the newest is not the pinned one — a store that has been
  provisioned by a NEWER checkout now hands that version to an OLDER one, which
  is problem 2 exactly, and silently.
* It cannot express "this project wants 0.5.1 deliberately".
* It has to be duplicated everywhere the store is read (cmake and shell today,
  hence the gate), and every copy is a chance to disagree — which is problem 1.
* The legacy unversioned `corrosion/{lib,share}` prefix cannot be attributed to
  any project at all, so no ordering rule can place it correctly.

## The governing principle: the path is OURS, so construct it

nano-ros decides where a provisioned tool goes. `nros setup` writes
`~/.nros/sdk/<tool>/<version>/` because the index said that version, and the
layout is nano-ros's own output — not something found in the environment.

So consumption must CONSTRUCT the path from the same two facts that produced it:

    ~/.nros/sdk/<tool>/<version-from-index>

Searching for it is discovering a fact we already know, and a search can return
something we did not install (the legacy unversioned prefix), something another
project installed (a newer version), or nothing at all — three wrong answers to
a question with a known right one. Every defect below follows from asking
instead of constructing:

* `file(GLOB)` + `COMPARE NATURAL ORDER DESCENDING` — a search, so it needs an
  ordering rule to make it deterministic;
* two implementations of that ordering (cmake and shell) — so it needs
  `check-cmake-corrosion-prefix` to keep them agreeing;
* the ordering being right and the answer still wrong, because a THIRD route
  (`add_subdirectory`) searches differently again.

A constructed path needs no ordering, no gate keeping two orderings in step, and
cannot be routed around: there is one spelling, and it either exists or it does
not.

## Proposed design: resolve the pin, never scan

**One rule, all tools.** A tool's location is a pure function of the project's
index:

```
~/.nros/sdk/<tool>/<version-from-[tool.<name>].version>
```

* **Hit** — use it. Report it (`nano-ros: <tool> <version> via <origin>`), which
  the Corrosion module already does and which is the only reason the 155/28
  split was visible.
* **Miss** — FAIL with the provisioning command, naming the pinned version and
  what the store holds. Never fall back to a different version: that is the
  silent substitution this issue is about.
* **Already-loaded** — accept only if it EQUALS the pin; otherwise fail naming
  both. A parent project supplying a different version is the mixed-version
  hazard, not a convenience.
* **No globbing, no sorting, no newest-first, no `find_package` search path.**
  Point `Corrosion_DIR` (and each tool's equivalent) at the constructed path
  directly. Delete the ordering rule and its gate: a constructed path cannot
  mis-order, and there is nothing left for two implementations to disagree
  about.

**Two projects then work by construction.** The store is version-keyed, so
`0.5.1-nros1` and `0.6.1-nros1` coexist; each checkout reads its own index and
finds its own. Neither can capture the other, and provisioning from one project
cannot change what the other resolves. That is true for every tool the index
pins, not just Corrosion.

**The legacy unversioned prefix retires.** `~/.nros/sdk/corrosion/{lib,share}`
belongs to no version and therefore to no project. Under exact-pin resolution it
is simply never named. Provisioning should stop writing it, and `nros doctor`
should report it as removable.

## What this makes unnecessary

* the newest-first ordering in `_nros_corrosion_prefixes` and its shell twin;
* `check-cmake-corrosion-prefix`, which exists to keep those two sorts agreeing;
* the `< 0.6.0` warning added in `1e1397b38` / softened in `1849bffb7` — with
  exact-pin resolution a legacy copy is never selected, so there is nothing to
  warn about. (It should stay until then: it is what surfaced this.)

## Evidence

* Prefix ordering itself is CORRECT and verified directly, so this is not an
  ordering bug:
  `0.6.1-nros1`, then `0.5.1-nros1`, then the flat prefix.
* Yet 155 leaf resolutions picked 0.5.1, through an `add_subdirectory` route
  ("Using Corrosion as a subdirectory") that never consults the prefix list.
  Those leaves hold no `Corrosion_DIR` cache — they resolve that way fresh.
* Nine workspace trees resolved correctly after their caches were cleared, which
  is why an earlier reading of this as "stale caches" looked right and was
  incomplete: it described the 9 and not the ~180.

## Scope note

The immediate breakage is contained (a warning, not a failure). This issue is
filed as a DESIGN change because the fix touches how every provisioned tool is
located, and RFC-0014 owns that surface. It should not be done piecemeal per
tool — that is how three routes appeared for one tool.
