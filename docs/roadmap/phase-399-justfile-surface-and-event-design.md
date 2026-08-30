# Phase 399 — the justfile has 332 verbs and about eight of them are an interface

**Status (2026-08-29). W1, W2 and W3 landed.** W1 extracted 200 gates into an
imported `just/check.just` (7,066 -> 4,141 lines, identical recipe set); W2
replaced `default` with a 28-line answer to "what do I type"; W3 corrected
AGENTS.md, where `ci-l1` was described as "the SAME tier the merge group runs"
after phase-396 W1 had made that false. Opened from "I see many justfile verbs, it's not memorizable
— find a better structure".

## Measured

```
root justfile        332 recipes, 7,066 lines
  of which check-*   203 recipes, ~3,041 lines with their doc comments (43%)
  ungrouped          211 of 332
just --list          215 lines, opening with a wall of check-*
21 module files      already split by PLATFORM (native, zephyr, freertos, …)
```

So the modules are not the problem — platform splitting already happened and
works. The problem is the root file, and it is one specific thing: **203
implementation gates share a namespace with the eight verbs a person types.**

## The distinction that makes this tractable

Nobody needs to memorise `check-cmake-find-program-shadowed`. It is not an
interface; it is a unit of `check-fast`, invoked by an aggregator, read only
when it fails — and when it fails it prints its own name, so discovery is by
failure, not by memory. Conflating "must be typeable" with "must be listed"
is what produced a 215-line `just --list`.

Three separable problems fall out, and they have different fixes:

| problem | fix | risk |
| --- | --- | --- |
| 203 gates in the root file | move to `just/check.just` | mechanical |
| 211 recipes ungrouped, so `--list` is a wall | `[group(…)]` + a real `default` | none |
| `--list` shows sentence FRAGMENTS | a short `#` line above each recipe | none |

That third one is worth seeing, because it makes the listing worse than empty:

```
check-cargo-config-tracked   # output); this is the discrimination the blanket rule cannot make.
check-cmake-find-program-shadowed   # libstdc++", host-idlc), so none failed loudly.
```

`just` shows the LAST comment line before a recipe. These recipes carry long
rationale blocks — which is right, and this repo's house style — so the line
`just` picks is the tail of a paragraph. The listing is not merely noisy, it is
*misleading*: every description is a fragment of a sentence about something
else.

## W1 — `import`, not `mod` — LANDED

`just/check.just`, brought in with **`import`** (which the root file already
does for `sdk-env.just`), NOT `mod`.

**`just/test.just` was NOT created**, and the reason is worth stating: there are
eleven `test-*` recipes against 203 `check-*`. Moving eleven recipes out of a
4,141-line file buys nothing and costs a second place to look. The complaint was
about a wall of 203, and that wall is gone.

This is the whole design decision, so state it plainly: `mod` would namespace
them (`just check fast`) and `import` keeps them flat (`just check fast`).
`mod` reads better in isolation and is wrong here, because the names are load-
bearing in about two hundred places — five workflow files, `run-gates-parallel.sh`'s
awk over `check-fast-serial`'s dependency list, CLAUDE.md, AGENTS.md, every
issue and phase doc that quotes a command, and the working memory of every agent
and person using this repo. A rename buys a nicer spelling and costs a
tree-wide edit plus a long tail of stale documentation.

**`import` gets the entire benefit — a 7,066-line file becomes ~4,000 — at zero
call-site cost.** Take it.

What stays in the root file, because it IS the interface:

- the tiers: `ci-l1`, `ci`, `ci-matrix`, `ci-matrix-nightly`, `ci-full`, `ci-l3`
- the aggregators whose dependency LISTS are the contract: `check`,
  `check-fast`, `check-fast-serial`, `check-build`
- `format`, `setup-*`, `claim*`, `doctor`

`check-fast-serial` in particular must not move: `scripts/build/run-gates-parallel.sh`
parses its dependency list out of the root justfile to build the fan-out set.
Moving it silently empties the parallel runner.

## W2 — a `default` that answers "what do I type" — PARTLY LANDED

`just` with no argument should print the eight verbs and what each costs, not
215 lines. The full list stays one flag away (`just --list`), and the gates stay
discoverable the way they are actually discovered — by name, when one fails.

**Landed:** `default` now prints 28 lines grouped by *when you would need it*,
instead of 215 opening with `check-*`.

**NOT landed, still open:** the 211 ungrouped recipes and the fragment
descriptions. `just --list` is unchanged — it still shows every gate described
by the tail of its rationale paragraph. Fixing it means a one-line `#` summary
immediately above each of ~200 recipes, with the long rationale moved above
that. Mechanical, safe, and 200 edits; it did not belong in the same commit as
a 200-recipe move, because then neither could be reviewed.

## W3 — write down why each event runs what it runs — LANDED

The tier ladder is documented; the EVENT mapping is not, and it has changed
three times this month (phase-395's per-event split, phase-396 W1 taking the
build tier off the merge group, and `check-build` moving to nightly). AGENTS.md
said the merge group runs "the SAME tier" as `just ci-l1` — untrue since W1, and
corrected in two places along with the tier table.

The mapping as it actually stands, with the reason for each choice:

| event | runs | why THIS and not more |
| --- | --- | --- |
| local pre-push | `just ci-l1` | the developer is the only one who can afford the compile tier without fixtures; it is stronger than the gate on purpose |
| push (any branch) | `check-fast` | cheap enough to run on every push; catches source-level breakage before a PR exists |
| pull_request | `check-fast` + `check-compile-smoke` | the gate must be fast or people stop opening PRs; smoke catches "does it still compile" without a fixture build |
| merge_group | `check-fast` + `test-unit`, plus `ci-l3` in `queue.yml` | tests `main` + PR, the commit that exists nowhere else; must be satisfiable by the job ITSELF (phase-396) |
| push to `main` | `post-submit`: `build-test-fixtures lane=tier2` → `ci-matrix`; `host-tests`: L2 | the only place fixtures are built in CI; heavy tier post-merge with a revert path, per the standard three-bucket split |
| schedule | `check-build`, `check-no-std`, platform matrix, `ci-matrix-nightly`, zephyr cells | needs provisioning no gating job has; latency here is free |

Two facts that belong in that table and are currently folklore:

- **`ci-l1` is NOT what CI runs.** CI runs `check-fast` + `test-unit`;
  `ci-l1` additionally runs `check-build` and `check-api-parity`. The local
  tier is deliberately stronger, and AGENTS.md now says so.
- **Exactly one CI job builds a fixture** — `post-submit`'s
  `build-test-fixtures lane=tier2`. Everything pre-merge is fixture-free by
  construction, which is why `ci-l1`'s "NO FIXTURES" claim has to hold and why
  `check-lane-contracts` enforces it.

## Not doing

- **Renaming the gates.** See W1.
- **Splitting `check.just` further** (by area: cmake, rust, ci, docs). Tempting
  and premature — one file of 203 gates that nobody types is not worse than four
  files of 50. Revisit if the file itself becomes hard to navigate.
- **Deleting gates.** Nothing here argues any of the 203 is unearned; the
  complaint was about the surface, not the coverage.

## What the extraction surfaced

`check-just-recipe-refs` followed `mod` but not `import`, so every imported
recipe read as UNDEFINED. It had been blind to `sdk-env.just` since that import
was added, and nobody noticed because no recipe body called one of those names.
Moving 200 gates into an imported file made it report all 200 missing at once —
a latent hole surfacing as a flood rather than as one wrong answer. Visible root
recipes went 133 -> 336. Five self-test assertions now pin the distinction,
including "`mod` is NOT read as an import", so it cannot be flattened by a later
tidy-up.

## One shape worth remembering

Three separate formatting failures in this phase had one cause: **a column-0
line inside a recipe is parsed as justfile syntax**, and a heredoc terminator
must be at column 0. `just`'s own body-dedent then fights any `sed` strip you
add to compensate. The form that works is `printf` with one argument per line —
indentation lives inside the quotes and survives nothing. The same trap appears
in YAML `run:` blocks (phase-396 W4 hit it), which is why it is recorded here
rather than in a commit message.
