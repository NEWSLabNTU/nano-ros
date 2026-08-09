---
id: 485
title: "`check-artifact-identity-budget` counted one crate as two, so a crate at 12
  identities passed a ceiling of 9"
status: resolved
resolved_in: phase-340
type: bug
area: build
related: [phase-340, phase-343]
---

## The defect

`scripts/check-artifact-identity-budget.sh` counted identities per crate with

```sh
identity_counts="$(printf '%s\n' "$triples" | awk '{print $1, $2}' | sort -u \
    | awk '{print $1}' | uniq -c)"
```

`uniq -c` collapses only **adjacent** duplicates, and glibc's `en_US.UTF-8`
collation ignores the space and the underscore when ordering. So on the real
tree:

```
nros 079babbedb254517          collates as   nros079babbedb254517
nros_board_common 2f72d54…     collates as   nrosboardcommon2f72d54…
nros_core 08f5cb3c…            collates as   nroscore08f5cb3c…
nros_cpp 756bf480…             collates as   nroscpp756bf480…
nros ecf7643749b10a78          collates as   nrosecf7643749b10a78
```

`nros_board_common`, `nros_core` and `nros_cpp` sort **between** two halves of
`nros`, whose hashes start `0/3/7/9/b` and `e/f`. `uniq -c` then emitted `nros`
twice — as `7` and as `5`.

## What it cost

* **The tree-wide ceiling stopped gating.** `awk '$1 > CEILING_IDENTITIES'`
  compared `7` and `5` against `9` and never saw the real `12`. A crate 33 %
  over the ceiling passed, silently, on every run since the gate landed
  (2026-08-07).
* **The headline number was wrong.** `worst crate 7/9` for a tree whose worst
  crate is 12.
* **It blocked a work item with a number that did not exist.** phase-340 item 8
  was parked on explaining a `worst crate` figure that read 5, then 6, then 7
  across sessions on an ostensibly unchanged tree, and its named next action was
  "diff the six `libnros_serdes-*.rlib` identities' feature sets". There was
  nothing to diff: `nros_serdes` measures **5**, in both the old counter and the
  new one, and it is not the worst crate. The "drift" was the RUN BOUNDARY moving
  as hashes changed between builds — a property of the hex digits, not of the
  tree.
* **It was one hash away from taking the gate down.** `crate_identities()` does
  `awk '$2 == c {print $1}'`, so a split budgeted crate returns TWO lines, and
  `[ "$n" -gt "$k" ]` on a two-line value is a bash syntax error. `nros_core`
  stayed contiguous only because its four hashes happen to start `0/4/6/9`. One
  starting with `e` or `f` would have split it.

## Fix

Count in one `awk` pass over an associative array keyed `(crate, hash)` — no
ordering to get wrong, no locale dependence:

```awk
{ if (!seen[$1 SUBSEP $2]++) n[$1]++ }
END { for (c in n) print n[c], c }
```

`LC_ALL=C sort` would also have worked and is a smaller diff. It was not chosen:
it fixes this pipeline by pinning an environment variable that the next author
has to know to keep, whereas the array cannot be broken by the environment at
all. The failure mode here is a gate that reads plausibly while being wrong, so
the fix should remove the possibility rather than configure around it.

**Guarded by a self-test that runs on every invocation**, on input engineered to
split under glibc collation and not under C:

```
nros 0aaaaaaaa
nros_board 1bbbbbbbb
nros fccccccccc      -> the counter must say `nros` has exactly 2
```

Standing rather than one-off, because **nothing about a wrong reading looks
wrong**: the old pipeline printed a smaller plausible number and exited 0.
Verified by reverting the counter to the broken idiom — the self-test reports
`'1\n1' identities for a crate that has exactly 2` and the gate exits 1.

## The numbers, first honest reading

The 2026-08-07 figures are **not comparable** to these; they were produced by the
broken counter.

| | recorded 2026-08-07 | true, 2026-08-10 |
| --- | ---: | ---: |
| `nros_core` (budgeted) | 8 | **4** |
| worst crate | 9 (`nros_serdes`) | **12** (`nros`) |
| worst identity, copies | 5 | 5 |

`12` is not a raised ceiling. It decomposes exactly:

```
2 workspace roots  x  2 R3 halves  x  3 feature identities  =  12
```

`nano-ros_23c15` and `nros_ws_runtime_16b35` are the roots (Wave 1's "22/22
leaves are workspace roots"); host `debug/deps` versus explicit
`x86_64-unknown-linux-gnu/debug/deps` is the R3 split phase-340 W3 made
universal. Nothing is unexplained, which is the precondition item 8 required
before any number moved. `nros_core` tightens 8 → 4 in the same edit.

## Sibling audit

Two other `sort | uniq -c` sites in the same script (axis 2, copies per
identity) are **correct and were left alone**: they count *identical* lines, and
identical lines are adjacent after any sort in any locale. The axis-1 bug needed
two *different* lines to be reduced to a common key first. That distinction is
now written at both sites so nobody "fixes" a correct one or copies the broken
idiom. `justfile:1483` counts identical `[SKIPPED]` strings — same reasoning,
and it is a display, not a gate.

## The rule this is an instance of

phase-340's own standing rule, turned on the gate that enforces it:
**re-measure an "N of M" claim before building on it.** The phase had already
paid for it three times (F3's "net loss", the impossible umbrella workspace, the
platform-grained key). This is the fourth, and the first where the unreliable
measurement was produced by the phase's own instrument.
