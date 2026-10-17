---
id: 1373
title: "A fresh `git worktree add` fails two fast gates, and one of them advises a
  recovery command that cannot run in a worktree"
status: resolved
type: bug
area: [tooling, testing]
severity: medium
found: 2026-09-18
related: [issue-0650, issue-0702, issue-0840, issue-1068, phase-478]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

## What happens

`git worktree add` does not populate submodules. Nothing else in the fast gate
tier minds that, because the tier is documented to run green on a pristine
worktree (`just/check.just:196-197`, issue 0650; `just/check/rmw.just:382-383` states the
same rule as the reason `zpico-config-keys` SKIPS instead of failing). Two gates
do mind, and they fail:

```
$ just check fast
FAIL capability-conditionals
  check-capability-conditionals: packages/rmw/zenoh/zpico-sys/zenoh-pico/include/
  zenoh-pico/system/common/platform.h is missing - run `git submodule update --init`
  for the zenoh-pico submodule. The socket-ABI rule cannot be checked without it.

FAIL xrce-vendored-versions
  check-xrce-vendored-versions: no vendored tree checked out - nothing verified.
  Run `nros setup --source micro-cdr --source micro-xrce-dds-client`.
```

The two failure sites, at the commit this was measured on (both scripts are
edited by the fix in the same commit as this file, so the line numbers move):

* `scripts/check-capability-conditionals.py:772-781` - the `else` arm taken when
  `PLATFORM_DISPATCH` (`scripts/check-capability-conditionals.py:79-80`) is
  absent, wired as a gate at `just/check/platform.just:219-220`.
* `scripts/check-xrce-vendored-versions.py:756-762` - the `if not versions` arm,
  wired as a gate at `just/check/rmw.just:440-441`.

Measured in a worktree created from `origin/main`:

    check-fast (parallel): 2 of 324 gate(s) FAILED
    busy 5737s over wall 818s at -P20 => 7.0x effective parallelism

Those two, and nothing else. Run standalone the scripts exit 2 and 1
respectively. `git submodule status` in that worktree shows all 20 submodules
with a leading `-` (uninitialised).

This is not a rare shape. The fast tier runs on every push through
`.githooks/pre-push` (issue 0840), and a worktree per task is the normal working
arrangement for the parallel sessions this repo runs - `.githooks/pre-push` says
so itself when it explains why a push from a LINKED WORKTREE sets `GIT_DIR`. So
every new worktree pays this, and pays it at the push. The wall time above is
what it costs to find out: 13.6 minutes on a cold worktree, because the tier's
slowest gates build (`cbindgen-headers` 817s, `api-parity` 785s) against an empty
target dir. The two that fail take 421 ms and 750 ms.

## Neither gate should be converted to a skip

`check-capability-conditionals.py:773-774` already decided this on purpose:
"zenoh-pico is a submodule; without it rule 4 has no source of truth. Say so
rather than passing over it (issue 0702)." The XRCE gate makes the same choice by
failing rather than reporting its two SKIP notes and exiting 0. Both are right -
the rules they carry (a socket-ABI selection, and a per-hop version derivation
across two build lanes, issue 1068) have no weaker form that is worth running.

So what is wrong is not the verdict. It is that the verdict names no command a
person in a fresh worktree can run.

## The XRCE gate's advice does not work where the gate fires

`scripts/check-xrce-vendored-versions.py:759` says

    Run `nros setup --source micro-cdr --source micro-xrce-dds-client`.

Measured in the worktree, verbatim:

```
$ nros setup --source micro-cdr --source micro-xrce-dds-client
Error: this `nros` does not belong to the checkout it is being run against.
    running: <other checkout>/packages/cli/target/release/nros
    checkout: <this worktree>
...
    Location: nros-cli-core/src/lib.rs:173:20
```

`setup` is a guarded verb (`packages/cli/nros-cli-core/src/stale_guard.rs:39-51`)
and the workspace check applies to it (`stale_guard.rs:75-84`), so the refusal is
correct and by design: a fresh worktree has no `target/` at all, `nros` on PATH is
whichever checkout was activated last, and that binary is foreign to this one. The
honest way out is `just setup-cli` - a full cargo build of the CLI - to satisfy a
gate that only ever wanted two directories checked out.

The same wrong advice is printed by the SKIP note at
`scripts/check-xrce-vendored-versions.py:656-660`.

What does work, in one command, with no build:

    git submodule update --init \
        packages/rmw/zenoh/zpico-sys/zenoh-pico \
        packages/rmw/xrce/xrce-sys/micro-cdr \
        packages/rmw/xrce/xrce-sys/micro-xrce-dds-client

## The fix

Both halves of option (c), because each covers a hole the other leaves.

**A verb.** `just setup-worktree` initialises exactly those three submodules and
nothing else. A verb rather than three paths in an error message because the paths
are the part that rots - `zpico-config-keys` already spells one of them in its own
skip note, so the tree carries two copies today and would carry four after this.

It initialises only submodules that are UNINITIALISED, and REPORTS an initialised
one whose checkout has moved off its pin rather than updating it. That is the rule
AGENTS.md states and `just post-rebase` (`justfile:3849-3852`) implements: a
`git submodule update` over a checkout someone is mid-edit on discards their work,
so it stays a human decision. Over an uninitialised submodule there is nothing to
discard, which is why this one can be a verb at all.

Deliberately NOT `git submodule update --init` with no paths: that would fetch
PX4-Autopilot, QEMU, NuttX and 14 others - gigabytes - for a gate that reads two
CMakeLists and one header.

**The messages.** Both gates now name `just setup-worktree` first and the explicit
`git submodule update --init <paths>` second, and the XRCE gate no longer advises
`nros setup --source`. A person who reads only the failure text can now act on it.

No gate fetches anything. `just check fast` still fails on a bare worktree; it now
fails with a command that works.

**A third gate had an opinion, and it was right.** The first push was refused by
`check-preconditions-provisioned`:

```
- gate tool recipe `just setup-worktree` is UNCLASSIFIED.
  A gate tells users to run it, so either `just setup` provisions
  it or it is deliberately manual. Add a row to TOOL_RECIPES.
```

That gate exists (`scripts/check-preconditions-provisioned.py:27-31`) precisely so
a new recipe named in a gate's failure text cannot be left unreachable from the
documented setup chain. `setup-worktree` is classified `manual`, which is the
class its own header defines (`check-preconditions-provisioned.py:37-38`) as "a
human decides (a submodule pointer move ...)". The alternative - running it from
`_setup-common` - would make every `just setup <scope>` on every host fetch three
submodules it may never build.

## What would close the class

Nothing here proves the list of three is COMPLETE - it is the set measured to be
needed today, and a 325th gate that reads a fourth submodule would restore the
original failure with a better message. The check that would close it is the
expensive one: run the fast tier in a genuinely pristine worktree in CI and require
green. That is a lane, not a gate, and it is not proposed here. Until then the
recipe names the two gates it exists for, so the next gate to grow a submodule
dependency has somewhere obvious to add itself.

## Resolution (2026-10-18)

**Measured first: nothing is red any more.** `just check fast` in a fresh
`git worktree add --detach` of `origin/main` (`6486216ab7`), with no submodule
initialised (`git submodule status`: 0 of 20 without a leading `-`), activated
and otherwise unprovisioned (no in-tree CLI): rc 0, 436 gates ran, 20 SKIPPED,
62 s wall. Both gates this issue names now take issue 1043's middle outcome.
Issue 1345 turned their hard FAILs into NOT VERIFIED through the skip ledger,
and the "a verb" half above gave them a remedy a worktree can run:

    [SKIPPED] capability-conditionals: NOT VERIFIED — rules 4/7/8 (socket ABI) need …/platform.h — `just setup-worktree`, or `git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico`
    [SKIPPED] xrce-vendored-versions: NOT VERIFIED — micro-xrce-dds-client, micro-cdr not checked out … — `just setup-worktree`, or `git submodule update --init …`

The remaining skips in that run belong to two groups. The first group needs a
submodule: `zenoh-feature-off-compile`, `zpico-config-keys` and `leaf-lockfiles`.
The second needs the in-tree CLI, a build tree, a west workspace or the FSP
tree, and each of those names `just setup-cli` or the tree it lacks. None
advises a command a worktree cannot run. None of them was fixed here, because
none of them fails.

**What closed the class.** The section above says nothing proved the list
complete, and that the check which would prove it is a lane:

> run the fast tier in a genuinely pristine worktree in CI and require green.

That check is now `just check pristine-checkout`
(`scripts/ci/pristine-checkout-fast.sh`). It builds a
`git clone --local --shared` of HEAD. That clone has the same tracked tree as a
fresh worktree, with every submodule uninitialised, and it registers nothing in
this repository's admin dir (issue 1465). The script overlays tracked edits and
untracked, non-ignored files, then runs `check fast` there as a person in a new
worktree would: no CI identity and no strict-skip knob. It requires rc 0 and
prints the `[SKIPPED]` block. It costs about 55 s here. It runs in `just ci gate`
after `check::fast`, and nightly / on dispatch in `gate.yml`'s `check-build` job,
which `check-default-gates-run-somewhere` requires. It is lane-exempt, because
inside the fast tier it would recurse.

It runs the tier rather than a census, and that choice is measured. Matching
the 20 submodule PATHS against each fast gate's recipe and scripts selects 17 of
456 gates, and misses BOTH gates this issue was filed about, because they build
the path from parts. Matching basenames selects 226 of 456, which is half the
tier on a heuristic.

Its normal-path selftest plants a recipe in the clone that requires content in
the first submodule `.gitmodules` names, and runs that one-gate list through the same runner. The selftest
requires that gate's own `===== FAIL` record. That proves the clone lacks
submodule content and that the runner's status reaches the verdict. The planted
recipe is then restored byte for byte.

Mutation proof (each reverted and restored with `cmp`):

| mutation | result |
| --- | --- |
| `capability-conditionals` back to the pre-1345 hard fail (`sys.exit(2)` on a missing header) | `pristine-checkout` rc 1, `===== FAIL (capability-conditionals, rc=2)`; the same mutation in a PROVISIONED worktree: `just check capability-conditionals` rc 0, which is why the ordinary fast lane cannot see this class |
| selftest probe looks for content in a directory that has some (`.`, not the submodule) | rc 1, `pristine-checkout SELFTEST FAILED` |
| runner cannot run the probe list (`--bogus` flag, exits 2) | rc 1, `pristine-checkout SELFTEST FAILED`, because a non-zero exit without the planted gate's FAIL record does not count |
| an UNTRACKED new script with a bare `grep -q` conditional | rc 1, `===== FAIL (grep-q-error-conflation)` |
| the same, with the overlay's intent-to-add (`git add -N` in the clone) removed | rc 0, so the offender is invisible |
| all restored | rc 0, `433 gate(s) ran … 23 SKIPPED`, `OK` |

The intent-to-add row is there because a real bare worktree caught what the
clone first missed. The first version of `pristine-checkout-fast.sh` was green
in its own clone while UNTRACKED. Then, in a fresh worktree at the commit, it
was red on two gates of its own: `grep-q-error-conflation` and
`store-source-readers` (the probe spelled a store-first submodule's checkout
path). Both gates ask `git ls-files` / `git grep`, which do not see an untracked
file. The overlay now marks overlaid files intent-to-add in the clone's own
index, so those gates see a new file as they will once it is committed. The
probe now reads its submodule from `.gitmodules` instead of spelling one.

On its first run the lane caught this change's own two reds,
`default-gates-run-somewhere` (no workflow yet) and `gate-selftests`, before
either was fixed. That shows it reports a red rather than swallowing one.

Not measured: the `gate.yml` step on a real runner. Dispatch it with
`gh workflow run gate.yml --ref <branch>`. A cold CI clone builds the fast
tier's compiling gates from an empty target dir, so expect minutes there, not
the 55 s measured on this host.

