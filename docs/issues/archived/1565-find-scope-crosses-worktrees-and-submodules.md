---
id: 1565
title: "A count is only as scoped as the tool that produced it — `find .` walks into `.claude/worktrees/*` and through gitlinks, so it answers a different question than `git ls-files`"
status: resolved
type: tech-debt
area: [tooling, testing]
severity: medium
found: 2026-09-29
resolved_in: 2026-10-01
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

## Resolution

The narrow gate was pursued, together with a helper that gives the
exclusion a single spelling. The CLAUDE.md documentation half had landed
with the filing; its pitfall line now also names both of these.

- **`scripts/lib/repo_walk.py`** is the scoped walk. `walk(root)` /
  `prune(dirpath, dirnames, root)` stop at every NESTED REPOSITORY: a
  directory holding a `.git` entry, which is a file for a linked worktree
  and for a submodule, and a directory for a nested clone. They also stop at
  the top-level `.claude/`. So one structural rule covers both halves of
  this issue (worktrees and gitlinks), with no name list.
  `python3 scripts/lib/repo_walk.py PATTERN…` is the same walk as a
  command, for ad-hoc analysis, which is where both miscounts here were
  made. It has a self-test that builds a worktree-file, a submodule-file, a
  nested clone and a non-top `.claude/`.
- **`check-repo-root-walk-scope`** runs on the fast line. It makes a
  committed walk ROOTED AT THE CHECKOUT ROOT name `.claude` in the same
  command, or go through `repo_walk`. The walks it reads are `find`, and in
  Python `os.walk`, `rglob`, a `**` glob and `glob(recursive=True)`, across
  every tracked `*.py`, `*.sh`, `*.just` and `justfile` outside
  `third-party/`.
  - **"The root" is derived per file, never listed by name.** A shell
    variable counts if it is assigned from `git rev-parse --show-toplevel`,
    or from `cd "$(dirname "$0")/<..×k>"` where k reaches the root from that
    script's own depth. A Python name counts if it is assigned from
    `__file__` with exactly as many `parents`/`.parent`/`dirname` steps as
    the file is deep. `{{justfile_directory()}}` counts, and so does `.` in a
    just recipe.
  - **Self-test (16 cases, normal path), failing as wanted:**
    - a recipe `find .`;
    - `find "{{justfile_directory()}}"`;
    - a show-toplevel variable across a line continuation;
    - a `dirname/..` root at the right depth;
    - `REPO.rglob` at `parents[depth-1]`;
    - `os.walk` over a dirname-chain root;
    - a cwd-relative recursive `glob`.
  - **Self-test, passing as wanted:**
    - the same walks pruning `.claude` or using `repo_walk`;
    - one `..` too few to reach the root;
    - an unbound `$root`;
    - a walk quoted in a docstring;
    - the negative controls the acceptance names: walks scoped to a build
      directory in just, sh and Python.
  - **Measured on the tree, 2026-10-01:** 369 files bind a checkout-root
    name, and none of them walks from it. The shell side was already clean
    (every repo-root `find` in `scripts/`/`just/` walks a temp tree, a build
    root or a subtree). Each Python recursive walk is either an index lookup
    or a `walk-ok` fallback over a synthetic tree. So the gate lands green,
    and it holds the line rather than fixing a live site.

**What stays out of reach, stated rather than guessed at:**

- a root passed in as a function parameter (`def f(root): os.walk(root)`);
- `.` in a `.sh` file, whose meaning depends on an earlier `cd`;
- ad-hoc analysis, which no gate reads.

For the last, the remedy is `git ls-files` for tracked content and
`repo_walk.py` for the rest, both named in CLAUDE.md.

**Re-measured with the helper.** In this worktree, `repo_walk.py
'*.contract.yaml'` and `git ls-files '*.contract.yaml'` both give 18. The
two now agree by construction, not by luck.
