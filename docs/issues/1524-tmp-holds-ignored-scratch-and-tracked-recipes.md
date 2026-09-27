---
id: 1524
title: "`tmp/` is both ignored scratch and TRACKED recipes, so the instruction that
  sends every session there does not warn that `rm -rf tmp` destroys committed
  files — and it has, twice"
status: open
type: bug
area: [tooling, docs]
severity: medium
found: 2026-09-28
related: [1513, 0196]
---

## What is wrong

CLAUDE.md tells every session:

> Temp files in `$project/tmp/` (gitignored), not `/tmp`

That is **true for new files** — `.gitignore:57` is `/tmp/`, and
`git check-ignore -v tmp/brand-new-scratch.log` confirms the rule fires. It is
also **incomplete in the way that matters**: ten files are TRACKED in that
directory, and a tracked file is exempt from every ignore rule.

```
$ git ls-files tmp/ | wc -l
10
$ git check-ignore -v tmp/migrate-app-main.py
(no output — tracked, so not ignored)
```

`tmp/collapse-*.sh` (eight) and `tmp/migrate-*.py` (two). And this is
DELIBERATE: `.gitignore:50`'s own comment says *"Recipes for repeated multi-step
ops live here too."*

So one directory serves two purposes — ignored scratch and committed recipes —
with nothing in the path, the name or the instruction telling them apart.

## Measured cost

A delegated agent followed the instruction, put its scratch in `tmp/`, cleaned up
with `rm -rf tmp`, and **deleted four tracked scripts**. It noticed and restored
them with `git checkout -- tmp` in the same session, so nothing was lost
permanently — but the recovery depended on the agent spotting it, and the report
it filed diagnosed the cause as *"`$project/tmp/` is NOT gitignored — CLAUDE.md's
claim is false"*, which is wrong in a way that would have sent the next person to
edit the wrong line.

The claim is right. The directory is not safe.

## Why the instruction is what makes it a trap

A session is told to treat `tmp/` as disposable, and treating a disposable
directory as disposable is how it gets `rm -rf`'d. Nothing on the way in says
"some of this is committed". The trap is not the tracked files and not the ignore
rule; it is the pairing, plus an instruction that names only one half.

## What closing it looks like — three options, and they are not variants

1. **Move the tracked recipes out** (e.g. `scripts/oneshot/` or
   `scripts/migrations/`), leaving `tmp/` purely disposable. The instruction then
   becomes true without qualification, and `rm -rf tmp` becomes safe — which is
   what every session already assumes. Cost: ten path changes, and any doc or
   recipe naming them moves too.
2. **Keep both purposes and make the split VISIBLE** — e.g. tracked recipes under
   `tmp/keep/` with a negated ignore rule, scratch anywhere else. Cheaper, but it
   preserves a directory whose safety depends on reading `.gitignore`.
3. **Correct the instruction only** — say that `tmp/` holds tracked recipes and
   must never be `rm -rf`'d. One line, no moves; leaves the trap armed for anyone
   who does not read it.

**Recommended: (1).** The other two ask every future session to hold a
distinction the filesystem could hold instead. It is also the only one that makes
the existing instruction true as written, and the instruction is the thing
sessions actually act on.

This issue takes (3) now as the stop-gap, because the warning costs a line and
the moves need an owner.

## Not this issue

The `/tmp/` rule appears TWICE in `.gitignore` (lines 51 and 57, with two
different comments above them). Harmless duplication, noted so the next reader
does not assume one of them is load-bearing and the other is not.
