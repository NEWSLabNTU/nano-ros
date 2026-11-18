---
id: 2711
title: "A checkout CLI with no resolver of its own falls through to an installed SDK's, and the refusal blames the pin instead of the missing build"
status: resolved
resolved_in: "the change that files this issue"
type: bug
severity: low
area: [cli, tooling, launch]
related: [0409, 1850, 0285]
found: 2026-11-18
---

**Home:** no phase. Reported by the AUO book session, worktree
`.claude/worktrees/i2581`.

## What happened

`workspace-system-required` failed in a fresh worktree with issue 0409's
refusal: the `nros-launch-resolve` it found was built from play_launch
`d9b79f2`, while the worktree's CLI was built from `eb8758a`. It read like a
shared binary out of date, and the first report asked for a per-checkout
resolver location.

That location already exists. The CLI's ladder
(`model_location::launch_resolver_rungs`, issue 1850) is:

1. `$NROS_LAUNCH_RESOLVE`;
2. this checkout's build, `<checkout>/packages/cli/nros-launch-resolve/target/release/`;
3. beside the running `nros` (the installed layout);
4. the store's `bin/`.

Measured in that worktree:

- `CARGO_TARGET_DIR` was unset, and the gate ran the worktree's own
  `packages/cli/target/release/nros`, built by `just setup-cli` minutes
  earlier;
- rung 2 was EMPTY: `just setup-cli` builds the CLI, not the resolver
  (`just setup-worktree` builds both);
- so the ladder answered from the store: `~/.nros/bin/nros-launch-resolve`,
  a symlink from 2026-10-26 into an installed nros4 SDK, built from
  `d9b79f2`.

The 0409 refusal was right to refuse a resolver from another pin. What it
said was wrong for this case: the fix was to build THIS checkout's resolver,
and nothing about a shared binary needed rebuilding. After
`just setup-launch-resolve` in the worktree, rung 2 answered and the gate
passed. `~/.nros` was untouched, so no other session was affected.

## Fix

`model_location::launch_resolver_fallthrough_note` asks the same ladder
whether the resolver that answered came from below an empty checkout rung.
If it did, the 0409 refusal ends with:

```text
This checkout has no `nros-launch-resolve` of its own (<rung 2 path> does not
exist), so the lookup fell through to <path> (<rung>), which another install
or checkout built. In this checkout, run:

    ./scripts/bootstrap.sh      (contributors: just setup-launch-resolve;
    `just setup-cli` builds the CLI only, `just setup-worktree` builds both)
```

When the resolver that answered IS this checkout's build, or there is no
checkout (an installed CLI), the refusal is unchanged. Unit test:
`a_fallthrough_past_the_checkout_rung_is_named`.

## Why not build it from `just setup-cli` too

`just setup-worktree`, the documented first step of a fresh worktree,
already builds this checkout's resolver: its dev-lane `cli` claim runs
`just setup-cli` then `just setup-launch-resolve` (`claim_cli` in
`scripts/lib/claim-provision.sh`). The worktree here ran `just setup-cli`
alone, which is the verb for rebuilding the CLI by itself, and the resolver
is a separate pyo3 build. So prevention is already on the road a fresh
worktree takes, and the refusal now names the step for the one that skipped
it.

## Not changed

Whether a checkout CLI should ever use a lower rung's resolver. A store or
installed resolver from the SAME pin produces the same models, and 0409
already refuses one from another pin, so falling through is safe; only the
message was missing.

Sweep: `git grep -n 'launch_resolver_bin_for_exe\|verify_resolver_pin' -- packages`
(the pin check is the one refusal of a found resolver; the not-found path
already prints every rung through `launch_resolver_missing_help`).
