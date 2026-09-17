---
id: 1373
title: "A fresh `git worktree add` fails two fast gates, and one of them advises a
  recovery command that cannot run in a worktree"
status: open
type: bug
area: [tooling, testing]
severity: medium
found: 2026-09-18
related: [issue-0650, issue-0702, issue-0840, issue-1068]
---

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
