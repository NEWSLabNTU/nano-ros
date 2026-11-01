---
id: 1964
title: "The pre-push `just check fast` went red once and green on an immediate retry, and which gate it was cannot be recovered"
status: wontfix
type: bug
area: [ci]
severity: low
found: 2026-10-18
resolved: 2026-11-01
related: [issue-0840, issue-1625, issue-1868, phase-505]
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — listed in that phase's "Issues homed here".

## What happened

On 2026-10-18 an agent worktree was created from `origin/main`, provisioned
(`just setup-worktree`, `setup-cli`, `setup-launch-resolve`), and ran
`just ci gate` green. About 1.5 h later, in one command, it ran
`git fetch origin && git rebase origin/main`, `git submodule update -q`,
`setup-cli` and `setup-launch-resolve` (both rc 0), and then
`git push -u origin fix/1898`. The `.githooks/pre-push` hook's
`just check fast` (issue 0840) refused the push:
`REFUSING to push — the fast gate tier is red`.

Minutes later the same push from the same worktree passed the hook, with
nothing changed in between. Two other pushes the same day, made the same way
from other worktrees, passed the first time.

Only the hook's last three lines were kept, and the worktree was removed
afterwards. So nothing records which gate was red, or whether the red came
from a gate or from the runner itself before the fan-out.

## Reproduction attempts (2026-10-18/19): none reproduced it

All runs were in a fresh linked worktree on this host. "Hook" means a real
`git push --dry-run`, which runs the pre-push hook. Every run had 456–458
gates and 0 red.

| Run | What | `check fast` wall time |
| --- | --- | --- |
| baseline | fresh worktree at `origin/main~30`, provisioned | green |
| `ci gate` | `just ci gate` at the rebased tree | `check::fast` ok. The run stopped at `test-unit`, on one unrelated test (`client_session_with_absent_locator_dials_backend_default`) |
| cycles 1–5 | reset to `origin/main~30` + a trivial commit, then rebase onto `origin/main` (~200 files), `submodule update -q`, `setup-cli`, `setup-launch-resolve`, hook ×2 | 41–47 s / 40–41 s |
| cycle 6 | the same, with 40 CPU hogs on a 20-CPU host | 207 s / 208 s |
| replay | the actual #1898 commit cherry-picked onto `aacfc3c1f8` (its likely base), fast green there, then rebased onto `af3bd5a8ad` (`main` at push time), hook ×2 | 149 s / 115 s |
| loaded ×2 | hook with 40 CPU hogs, no rebase | 130 s / 137 s |

The data show that some gates depend on state, but none of them flipped
its verdict:

- `declared-qos-registration` compiles `nros-node` three times into
  `tmp/declared-qos-registration/` on the first run after every rebase. The
  first run took 9–13 s (27 s under load); the second took 0.8 s.
- `cbindgen-headers` (`cargo run`) took 7–8 s on the first run and 1.3–2.3 s
  on the second.
- Under load, `knob-ends`, `no-board-init` and
  `platform-port-single-provider` took about twice as long on the first run.
- `runner-sweep-eviction-order` took 20 s once, against its usual 0.8 s, with
  the same CPU time. That is a wait (I/O or a lock), not work.

No gate in the lane writes to the repository state that this worktree shares
with the main checkout (refs, config, packed-refs). The `origin/main`
consumers use `merge-base`, so a concurrent fetch in another checkout that
moves `origin/main` does not change their answer.

Two candidates were not ruled out:

1. The red came before the fan-out. `run-gates-parallel.sh` can exit
   non-zero before any gate runs: in gate-list derivation, in the skip-ledger
   self-test, or in its `cargo --version` toolchain completion (issue 1868's
   rustup race). The hook then prints only `recipe fast failed`, with no
   `===== FAIL (<gate>)` block. This was checked on purpose: with
   `NROS_GATE_LANE` set to an unknown lane, the hook's last three lines look
   exactly like the ones that were kept.
2. Contention with another session on the shared host, which these runs did
   not recreate. Load on the host reached 62 during the attempts.

## What landed

This issue lands a diagnostic. It is not a fix, because the cause is
unknown. The hook now runs `just check fast` through
`scripts/lib/prepush-log.sh` (`nros_prepush_run_logged`), which writes two
files under
`${NROS_PREPUSH_LOG_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/nros-pre-push}/`:

- `<UTC>-<pid>.log`: the full combined output, a header (HEAD, `origin/main`,
  load average) and the exit status.
- `<UTC>-<pid>.gates.tsv`: one row per gate (`name rc wall_ms cpu_ms`),
  through the runner's existing `NROS_GATE_TIMINGS_FILE`.

On a red, the hook prints where the logs are. The directory is outside every
checkout, so the logs survive `git worktree remove`, and
`check-hook-repo-side-effects` (which compares the reachable repositories
byte for byte) is not affected. The newest 50 runs are kept. If the log
cannot be written, the gates still run and the verdict is unchanged.

The fast gate `just check pre-push-log` (`tests/pre-push-log-tests.sh`) covers this
code. It passed a 9-mutation table, after the table found that the first
version of the test was vacuous: the log header echoed the command line, so
it satisfied the stream assertions on its own.

## What would close it

The next time the hook goes red, read the newest
`~/.local/state/nros-pre-push/*.log` and `*.gates.tsv`. They name the gate,
or show that no gate ran. Then file or fix that cause, and close this issue
against it.

## Resolution (2026-11-01) — wontfix, cannot reproduce

Closed without a cause. Six reproduction attempts found nothing, and in the
two weeks since the diagnostic landed no red has been traced back here. The
maintainer chose to close it rather than keep it open waiting for an event
that may not come again.

The diagnostic stays. The hook still writes its full output and per-gate
table under `~/.local/state/nros-pre-push/`, so if the pre-push
`just check fast` goes red again, read the newest `*.log` and `*.gates.tsv`
there and file a new issue against the gate (or the pre-fan-out step) they
name, citing this one.

That diagnostic only collects where the hook is installed. A checkout that
has not run `just setup-hooks` has no `core.hooksPath` and no
`.git/hooks/pre-push`, so its pushes run no gates and leave no log. That was
true of the checkout this issue was closed from.
