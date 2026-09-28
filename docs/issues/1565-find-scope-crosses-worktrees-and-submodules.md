---
id: 1565
title: "A count is only as scoped as the tool that produced it — `find .` walks into `.claude/worktrees/*` and through gitlinks, so it answers a different question than `git ls-files`"
status: open
type: tech-debt
area: [tooling, testing]
severity: medium
found: 2026-09-29
related: [1465, 1336, 1555, 1564, 0196, 1452]
---

## What

`scripts/check-no-tracked-file-find.sh` already carries the rule *"Never `find`
for a file git tracks. Use `git ls-files`."* Its rationale is entirely
**performance**, and it is a good one — measured 570x, with the `find` at 0% CPU
the whole time because it was I/O-starved walking build trees, plus the trap that
`-prune` does not fix it (`find` must stat a directory to decide to prune it).

**The correctness half of the same rule is unwritten, and it is the half that
costs accuracy.** In this repository a `find` does not merely take longer than
`git ls-files` — it answers a DIFFERENT QUESTION, because it walks into two
places that are not this tree:

- **`.claude/worktrees/*`** — parallel agent sessions work in linked worktrees
  here, each with its own build output. `find` sweeps all of them.
- **submodules** — `git ls-files` stops at a gitlink; `find` walks through it into
  another repository entirely.

Slower is a cost. Wrong denominator is a defect. Two rules that happen to share
one remedy.

## Measured, both directions, on 2026-09-29

Both errors happened the same day, in opposite directions, from the same cause.

**Worktrees — a 19x overstatement.** Counting the generated declared-QoS header:

```
ls .claude/worktrees | wc -l                                              19
find . -name 'nros_declared_qos_generated.h'                              19
find . -name 'nros_declared_qos_generated.h' -not -path './.claude/*'      1
git ls-files | grep -c nros_declared_qos_generated.h                       1
```

18 of the 19 were other agents' build output. The live consequence: two of those
headers were reported as "production headers, both `refused`" in support of a
claim about this tree, and they were under
`.claude/worktrees/agent-a8dd0242b7f667453/`. An earlier count in the same
investigation ("16 of 18 headers carry rows") was the same error. The reporter
caught it only because one `"refused"` line contradicted their own number.

**Submodules — 17 became 44.** Counting contract sidecars:

```
git ls-files '*.contract.yaml' | wc -l                                    17
find . -name '*.contract.yaml' -not -path './.claude/*' | wc -l           44
comm -13 <(git ls-files '*.contract.yaml' | sed 's|^|./|' | sort) \
         <(find . -name '*.contract.yaml' -not -path './.claude/*' | sort) \
  | grep -c 'third-party/play_launch'                                     27
```

27 of 27 gap files belong to `play_launch`. 17 + 27 = 44 with nothing unexplained
on either side — two people were counting two different repositories and both
were internally correct. A denominator of 44 would tell a reader this tree holds
44 contracts; it holds 17.

## Why this is not fixed by widening the existing gate

Considered and rejected. `check-no-tracked-file-find.sh`'s `TRACKED` list is a
~10-name regex and `contract.yaml` appears in it zero times, so the obvious move
is to add it. That would be wrong: `nros sync` synthesises a leaf's contract into
a generated dir, so a `.contract.yaml` can legitimately be untracked build
output — exactly the `$staged` case that file's `NO_INDEX` arm exists to protect,
and whose own comment says twice that *"a gate that demands an impossible fix
gets disabled."* The allowlist is narrow on purpose and should stay narrow.

Widening by EXTENSION is worse still: the tree tracks 2776 `.md`, 630 `.toml`,
528 `.xml`, 363 `.py`, and legitimate scans for untracked artifacts of those
shapes exist.

## What the rule is

When the question is **"what does this tree contain"**, the tool is `git
ls-files` (or `git grep`), because that is the tool whose scope IS this
repository. `find` answers **"what is on this disk under this path"**, which in a
checkout with agent worktrees and 20 submodules is a strictly larger and
differently-shaped set.

When `find` is genuinely required — untracked artifacts, build output, a clean
recipe — scope it to the build directory, and never to `examples/`, `packages/`
or `.`.

## Fix direction (not decided)

The rule above is documentation, and a pointer is going into CLAUDE.md's pitfall
index. What is NOT decided is whether it should also be mechanical, and the
honest answer is that it is harder than it looks:

- a gate forbidding a bare `find .` in `scripts/**` / `just/**` is cheap, but the
  two errors this issue records were made in **ad-hoc analysis**, not in
  committed scripts, so such a gate would not have caught either;
- the shape that WOULD have caught them is a rule about what a stated count may
  be derived from, which is not statically checkable.

So the candidate worth measuring is narrower: require that any committed script
walking the repo root excludes `.claude/worktrees` explicitly, since that path is
never a legitimate subject for an in-repo measurement. 10 scripts already mention
`worktrees`; the rest have not been audited, and whether any of them measure
rather than merely traverse is unknown.

## Acceptance

Undecided by design — the documentation half lands with this filing. If the
narrow gate above is pursued, its acceptance is that a committed script rooted at
the repo root and not excluding `.claude/worktrees` fails, with its negative
control being a script that scopes to a build directory.
