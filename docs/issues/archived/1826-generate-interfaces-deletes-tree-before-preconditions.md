---
id: 1826
title: "`just generate-interfaces` deleted the tracked interfaces tree before the generator ran, so any failure left the checkout unbuildable"
status: resolved
type: bug
severity: medium
area: [codegen, tooling]
related: [1821]
found: 2026-10-12
---

## Measured

While fixing issue 1821, `just generate-interfaces` was run with a CLI that its
own stale check refused (`in-tree nros CLI is STALE`). The recipe had already
run `rm -rf generated/humble`, so the run ended with all six tracked crates
under `packages/interfaces/generated/humble/` deleted. The root `Cargo.toml`
names them as workspace members, so every later `just` invocation failed
before reaching its recipe:

```
error: backtick failed with exit code 2
  ——▶ just/check.just:45:21
45 │ HOST_UNCHECKABLE := `bash scripts/build/embedded-only-members.sh`
```

That script greps a member manifest that no longer existed. Even
`just setup-cli`, the remedy the stale-CLI message names, could not run. The
recovery was `git checkout -- packages/interfaces/generated`.

The delete is deliberate. A full re-emit is how a message removed upstream
stops leaving a stale file behind. But it ran before anything that could fail:
the CLI's stale check, the generator itself, the closure check.

## Fix

The recipe generates into a staging dir and swaps it in only after the
generator succeeded and every crate the root workspace names is present. The
staging dir is `packages/interfaces/generated/.humble.staging`: a sibling of
the real tree at the SAME depth, because the generator writes path dependencies
relative to its output dir. A first attempt staged under `build/`, and its
manifests named `packages/packages/core/...`. The staging dir is removed on exit
and gitignored in case a run is killed.

Measured both ways in a fresh worktree:

- **Stale CLI** (a CLI source edited after `just setup-cli`): exit 1, with the
  tracked tree untouched and no staging dir left.
- **Current CLI:** exit 0, the regenerated tree byte-identical to the committed
  one (`git status` clean), and no staging dir left.

Sweep: `git grep -n -E "rm -r[f ]|rmtree" -- justfile 'just/*.just' scripts`,
filtered to generated, binding and golden paths. Every other hit clears
gitignored build output or a user-side `generated/` tree. This was the only
recipe that deleted a TRACKED tree ahead of its regeneration.
