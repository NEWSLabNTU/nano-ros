---
id: 1518
title: "The wrong entry count propagated from issue 1288's title into RFC-0098,
  and 1288's `related: [1108]` points at an archived path"
status: open
type: tech-debt
area: docs
severity: low
found: 2026-09-27
related: [1288, rfc-0098, phase-445]
---

## What this is

Two residues of one wrong number, left deliberately unfixed when issue 1288 was
extended (that edit was scoped to the issue itself).

**1. RFC-0098 repeats the count.** `docs/design/0098-generated-leaf-build-config.md`
line 242, in the phase-445 W5 amendment:

> …is still false for the eight Rust west entries: the entry is derivable, the

There are **seven**, and there were seven on the day 1288 was filed — the same
`git ls-files 'examples/workspaces/*/src/*entry*/Cargo.toml'` query against that
commit returns the same seven paths, and 1288's own table listed seven while
only its title said eight. Today:

```
features/src/zephyr_rust_{lifecycle,params,qos}_entry
realtime-rust/src/zephyr_entry
rust/src/zephyr_entry
rust/src/zephyr_entry_robot1
safety/src/zephyr_rust_safety_entry
```

1288's title has been corrected (it now counts the 15 hand-written entries
across all languages, which is the number that matters for the migration). The
RFC citing it has not, so the wrong number now has one live carrier and reads as
corroboration.

**2. `related: [1108]` resolves to nothing.** Issue 1108
(`templates-materialize-dead-entry-pkgs`) is resolved and lives at
`docs/issues/archived/1108-templates-materialize-dead-entry-pkgs.md`. A reader
following 1288's `related` list at the unarchived path finds no file. The
reference is still *correct* — 1108 says what 1288 leans on it for — it is just
unfollowable.

## Why file it rather than fix it in passing

The count is the interesting part. It was wrong in the title from the first
commit, the body disagreed with the title from the first commit, and the number
travelled into a design document anyway — which is what CLAUDE.md's "fix the
CLASS, not the reported site" is about. Worth asking once, while the evidence is
fresh:

- does any other doc cite a count of these entries? (Sweep for the spelled-out
  numbers and for `*_entry` counts, not just for "eight".)
- is a spelled-out count in prose worth carrying at all, when
  `check-declared-fact-carriers` exists precisely for facts that drift? A
  sentence that says "the Rust west entries" needs no number.

## Acceptance

- RFC-0098 line 242 states the right count, or no count.
- 1288's `related` reaches 1108 (archived path, or whatever spelling the issue
  index uses for an archived reference — check how other issues cite archived
  ones before inventing a form).
- A sweep recorded for other carriers of the same count, with its command, so
  the next person can re-run it rather than re-derive it.
