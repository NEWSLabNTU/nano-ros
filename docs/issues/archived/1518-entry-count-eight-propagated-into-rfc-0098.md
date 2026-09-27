---
id: 1518
title: "The wrong entry count propagated from issue 1288's title into RFC-0098,
  and 1288's `related: [1108]` points at an archived path"
status: resolved
type: tech-debt
area: docs
severity: low
found: 2026-09-27
resolved: 2026-09-27
resolved_in: "phase-470 W1 — the number was DROPPED at all five carriers, not corrected; `related: [1108]` needed no change because the bare id IS the convention"
related: [1288, 1511, rfc-0098, phase-445, phase-470]
---

> **RESOLVED 2026-09-27 (phase-470 W1).** The sweep found **five** live
> carriers, not the one this issue assumed — see "The sweep" below for the
> command and the verdict on each. Every one had the count **removed** rather
> than corrected, on this issue's own argument: "the Rust west entries" cannot
> drift. `related: [1108]` was left exactly as it was, because checking the
> convention before inventing a form showed the convention was already being
> followed.

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

---

## Resolution (2026-09-27, phase-470 W1)

### The count was wrong from the first commit, confirmed both ways

```
$ git ls-files 'examples/workspaces/*/src/*entry*/Cargo.toml' | wc -l
7
$ git ls-tree -r --name-only <the commit that filed 1288> \
    | grep -cE '^examples/workspaces/[^/]+/src/[^/]*entry[^/]*/Cargo\.toml$'
7
```

Same seven paths, both trees. Not drift: **the number was never measured**, and
the filing commit is the one that wrote both the wrong count and the table that
contradicted it.

### The sweep

```
git grep -nEi '\b(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|[0-9]+)\b[^.]{0,50}\b(west|hand-written|zephyr|rust)?[^.]{0,20}\b(entr(y|ies)|_entry)\b|\b(entr(y|ies)|_entry|west application)\b[^.]{0,50}\b(seven|eight|fifteen)\b' \
  -- 'docs/**.md' 'book/**.md' '*.md'
```

Deliberately not a grep for `eight`: the word appears ~80 times in the tree for
unrelated things (nightly runs, cache entries, `[python.*]` rows, pip layers),
and the two other spellings of this count — `seven` and `fifteen` — would each
have been missed by it. Reading the hits by hand is the point; the regex only
gets the candidate set down to something readable.

**Five live carriers, not one:**

| carrier | was | now |
| --- | --- | --- |
| `docs/design/0098-generated-leaf-build-config.md:242` | "the eight Rust west entries" | "the hand-written Rust west entries" |
| `docs/issues/1288-*.md` opening sentence | "Eight entries are still hand-written" | "These entries are still hand-written" |
| `docs/issues/1288-*.md` acceptance line | "a west build of each of the eight" | "a west build of each of them" |
| `docs/issues/1288-*.md` fix section | "Then delete the eight packages" | "Then delete the hand-written packages" |
| `docs/issues/1511-*.md` related-reading list | "the eight Rust Zephyr entries (fifteen packages in all)" | "the hand-written Zephyr west entries (fifteen packages, of which seven are Rust)" |
| `docs/roadmap/phase-445-*.md` W5 record | "The eight Rust Zephyr entries and every…" | "The hand-written Rust Zephyr entries and every…" |

**Three of the five were inside 1288 itself.** That corrects a claim 1288's own
2026-09-27 re-measurement made about its own file: it said "only the title said
eight", when the title, the opening sentence, the acceptance and the fix all did
— the table was the only thing in the file that had counted. Fixed there too, so
the correction does not itself carry a wrong statement about where the wrong
number lived.

### Dropped, not corrected — at every site

None of the five sentences needs the count to make its argument. RFC-0098's is
"D6 does not hold on Zephyr yet", 1288's are "these are still hand-written",
1511's is "this is the migration I defer behind", phase-445's is "these lost
their manifest deployment keys". A number in any of them is a fact a reader must
re-verify and a maintainer must re-measure, buying nothing. The one place the
count is KEPT is 1288's re-measurement section and this file, both of which are
*about* the number — there it is the subject, not decoration.

### `related: [1108]` — the convention was already being followed

Checked before inventing a form, as the acceptance asked:

* `related:` takes **bare ids** (plus `rfc-NNNN` / `phase-NNN` / `issue-NNNN`
  tokens). Across the corpus there is **no** archived-path spelling in any
  `related:` list.
* **86 open issues already cite at least one archived id** as a bare number
  (0259 → 0403/0404, 0941 → 0940/0949/0950, 1040 → 1035/1021/0952/1030/1071, …).
  A form that made archived-ness visible in `related:` would therefore be a new
  form used by exactly one file, and would go stale the moment any cited issue
  is archived.
* The ledger resolves it: `just issues --id 1108` returns the row (`#1108  done
  …`) from `archived/`, because `--id` searches both. `related:` is an id list,
  and an id is followable.
* 1288 **already** spells the archived path in prose, which is the repo's way of
  making an archived reference clickable:
  "**1108 — resolved and ARCHIVED** (`docs/issues/archived/1108-templates-materialize-dead-entry-pkgs.md`)".

So the issue's premise was narrower than it read: nothing is broken, and a
reader hand-building `docs/issues/1108-*.md` from a `related:` id would find
nothing for 86 other issues too. **No change made** — the alternative was to
invent a spelling, which is the thing the acceptance warned against.

### A near-miss worth recording, so the next sweep does not "fix" it

`docs/roadmap/phase-383-colcon-like-builder.md:67` ("fifteen entry packages in
one workspace alone") and `docs/design/0065-colcon-like-workspace-builder.md:778`
("for `examples/workspaces/rust` that is 15 entry packages plus one 19-member
root manifest") look like the same residue and are **not**. They count ALL entry
packages in the `rust` workspace across every platform, pre-migration, and were
measured: at phase-383's own creation commit that directory held exactly 15
`*entry*/package.xml` (`esp32`, `freertos`, `nuttx`, `threadx`, two `zephyr`,
nine `native`). Both are correct history about a different set. That 15 and
1288's 15 are unrelated coincidences.

Also not this class: `docs/issues/1309-*.md`'s "five of its eight hand-written
entries" (the `HOST_UNCHECKABLE` list, issue 1315) and
`docs/issues/archived/0421-*.md`'s "seven under `examples/zephyr/rust/`" (the old
per-example leaves, not workspace entries).

### Does this warrant a gate?

**Not on this evidence, and the sweep is the argument against one rather than
for it.** `check-declared-fact-carriers` is for a fact with a MEASURABLE
computation a gate can re-run and compare. This one has no stable subject: the
five carriers said "Rust west entries" (7), 1288's title says "hand-written
workspace entries" (15), and the two near-misses say "entry packages in one
workspace" (15, a different 15) — three different sets that a regex cannot tell
apart, which is exactly why the sweep had to be read by hand. A gate keying on
"a spelled-out number near the word *entry*" would have flagged ~30 correct
sentences to catch five wrong ones, and CLAUDE.md's own record is that a gate
whose reach is wider or narrower than its rule is the defect (issue 0196's
shape, and 1131 hitting both directions at once).

The durable fix is the one applied: **carry no count where the argument does not
need one.** All five sites now read as sentences that cannot drift, so there is
nothing left for a gate to check. If a future doc needs the number as its
subject, it should quote the `git ls-files` command beside it — which is what
1288's re-measurement section and this file do.
