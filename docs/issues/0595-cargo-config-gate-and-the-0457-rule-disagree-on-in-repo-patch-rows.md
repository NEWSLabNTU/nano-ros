---
id: 595
title: "`check-cargo-config-tracked` and issue 0457's rule disagree about a config holding only in-repo `# nros-managed` rows — tier 1 is red on main"
status: open
type: bug
area: build
related: [issue-0457, issue-0463, phase-351, phase-341]
---

## Symptom — `just ci` stops here, on a clean checkout of `main`

```
check-cargo-config-tracked: pure sync-output cargo config IS tracked:
  examples/threadx-linux/rust/talker/.cargo/config.toml
  examples/threadx-linux/rust/action-client/.cargo/config.toml
  examples/threadx-linux/rust/service-server/.cargo/config.toml
  examples/threadx-linux/rust/listener/.cargo/config.toml
  examples/threadx-linux/rust/service-client/.cargo/config.toml
  examples/threadx-linux/rust/action-server/.cargo/config.toml

  These hold nothing but sync's own include + [patch.crates-io] block, so
  they are recreated by `nros sync` and only churn in git. Untrack with:
      git rm --cached <path>
error: Recipe `check-cargo-config-tracked` failed
error: Recipe `ci` failed
```

Not branch-local. `origin/main` tracks all six `config.toml` and no
`nros-board.toml` beside any of them; the branch that hit this touches none of
those paths (`git diff origin/main...HEAD -- examples/threadx-linux/rust/` is
empty). **Tier 1 cannot complete on `main` as of 2026-08-15.**

## What the files contain

Everything that is not a comment, in all six (paths differ):

```toml
include = [ "../../../../../nros-patch.toml"]
[patch.crates-io]
nros-board-threadx-linux = { path = "../../../../packages/boards/nros-board-threadx-linux" }  # nros-managed
nros-platform = { path = "../../../../packages/platform/nros-platform" }  # nros-managed
```

Four content lines in a 27-line file; the rest is authored prose explaining why
there is deliberately no `[build] target` (ThreadX-Linux is a host binary, and a
literal `x86_64-unknown-linux-gnu` here read as a cross-compile on every other
host).

## The disagreement

`scripts/check-cargo-config-tracked.sh:37` decides a file has no authored
content when every line is blank, a comment, `include = `, the
`[patch.crates-io]` header, or an `# nros-managed` entry:

```sh
grep -qvE '^\s*$|^\s*#|^include = |^\[patch\.crates-io\]|# nros-managed\s*$' "$1"
```

So a config holding *only* in-repo managed rows is "pure sync output" and must be
untracked.

CLAUDE.md, recording issues 0457/0463, says the opposite for exactly this
content:

> **IN-REPO rows** (`nros-log`, board crates, `mps2-an385-pac` — relative paths,
> identical in every checkout) **stay INLINE in the tracked `config.toml`**,
> tagged `# nros-managed`; only `generated/` rows go to the GITIGNORED sidecar.

and records the cost of getting it wrong: 0457 moved the whole set to the
sidecar and "stranded every leaf on `no matching package named
'mps2-an385-pac'`, an in-repo patch a clone needs."

Both rules are defensible and they cannot both apply here. Following the gate
untracks a `[patch.crates-io]` that redirects `nros-board-threadx-linux` and
`nros-platform` to in-repo paths — the 0457 shape exactly. Following CLAUDE.md
leaves tier 1 red.

## The escape hatch does not reach these leaves

`includes_committed_projection()` (`:150`) exempts a config whose `include` names
a **tracked** `nros-board.toml` — the board `cargo_config` projection phase-341
W2 introduced and phase-351 extended. Migrated leaves have one, e.g.
`examples/workspaces/rust/src/esp32_entry/.cargo/nros-board.toml`.

These six cannot get one: `packages/boards/nros-board-threadx-linux/nros-board.toml`
declares `[[board]]`, `[board.entry]` and `[board.capabilities]` and **no
`cargo_config`**, so `nros sync` writes no projection. The only exemption the
gate offers is unreachable for this board.

## Why now

`check-cargo-config-tracked.sh` did not change in the pull that surfaced this
(last touched by `04ac80ed5`, phase-351 W3). The pull did land `1d0920301`
(phase-351 W5, "the board rung reaches cargo — from the invoker, not the leaf")
and `e63d88d25` (W6, "retire the old path"), which moved which leaves carry a
projection. Whether these six were meant to gain one, or were meant to be
untracked, or were meant to stay as they are with the gate relaxed, is a
phase-351 question — hence this issue rather than a guessed fix.

## Three candidate resolutions, none obviously right

1. **Give `nros-board-threadx-linux` a `cargo_config`** so sync projects a
   tracked `nros-board.toml` and the existing exemption applies. Most consistent
   with phase-351's direction; needs to be true content, not a stub written to
   satisfy a gate.
2. **Teach the gate that in-repo `# nros-managed` rows ARE authored-equivalent**
   — they are the thing a fresh clone cannot resolve without, which is the same
   test the gate applies to a `[build] target`. This is the reading CLAUDE.md
   already documents, and would make the `has_authored_content` predicate agree
   with 0457.
3. **Untrack the six** and accept that these leaves need `nros sync` before they
   resolve. Cheapest, and the one the gate's message prescribes — but it also
   deletes 23 lines of authored rationale per file that sync will not
   regenerate, and it is what 0457 was filed about.

(2) looks closest to the documented rule, but the choice belongs to whoever owns
the projection contract.

## Impact

Every `just ci` on `main` stops before the test sweep, so tier 1 is currently
unrunnable without `NROS_SKIP_*` bypasses — which is the same class of pressure
that produces museum binaries. Whichever resolution is taken should land with a
note in phase-351, since the gate's exemption is that phase's mechanism.
