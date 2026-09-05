---
id: 1074
title: "`check-cargo-config-tracked` reports a deleted package's leftover config as an untracked authored one — and its remedy would resurrect it"
status: resolved
type: bug
area: build
severity: medium
found: 2026-09-05
related: [0587, 0463, phase-338, phase-383]
---

# A gate that tells you to commit a corpse

## What it printed

On a developer machine, blocking `just build-test-fixtures` (which depends on
`check::fast`):

```
check-cargo-config-tracked: hand-authored cargo config NOT tracked:
  examples/qemu-arm-freertos/rust/action-server-entry/.cargo/config.toml
  examples/qemu-arm-nuttx/rust/service-client-entry/.cargo/config.toml
  … 12 in total
```

The remedy that follows from the gate's own rule — a config with authored
content must be tracked — is `git add -f`. **That would have committed config
files for packages deleted a month earlier.**

## What those files actually were

`git ls-files` on each package directory returns **nothing**. The packages were
removed by `ab486a8db` (phase-338 W2, 2026-08-05, *"collapse the 18 `-entry`
packages into their node packages"*), and later by phase-383 W10.a for the
workspace migrations — one of whose commit subjects is *"the bridges migrate,
and the shape cannot come back"*.

What survived on disk is entirely ignored residue: `Cargo.lock`, `generated/`,
and `.cargo/config.toml`.

**The mechanism is one sentence:** `git rm` cannot delete a gitignored file, and
every leaf `.cargo/config.toml` is gitignored (`.gitignore:111`, blanket — which
is the whole reason this gate exists). So deleting a package leaves its config
behind, in a directory that is no longer a package, and a FILESYSTEM walk finds
it.

The content really is authored — it was, when the package existed and the file
was tracked. The gate's content test is correct. What it lacks is any notion
that the package around the file is gone.

## Why it survived a month

**It cannot fire on CI.** A fresh clone has no residue, so the gate is green
there. It only fires on a machine that once built the old packages — so the
signal appears exactly where it is least likely to be read as a repo-wide fact,
and looks like local mess rather than a gate defect.

Same shape as issue 0587, which made this gate demand the DELETION of
documentation: both are the gate reasoning correctly about content while missing
what the file is *for*.

## Fixed 2026-09-05

`package_is_deleted()` — a config whose package directory has no tracked files
is residue, and is skipped. The discriminator is cheap and cannot mistake a live
leaf for a dead one: a live package always has at least a tracked manifest.

**The skip is REPORTED, not silent.** A skip nobody can see would be the same
defect one layer over — the residue would sit forever and the next person
wondering why a deleted example still has a `generated/` tree would have nothing
to read. The gate now prints the count and the remedy that is actually correct:

```
  note: skipped N cargo config(s) whose package has no tracked files — residue
  of a deleted package (`git rm` cannot remove a gitignored file, and every leaf
  config is gitignored). Not a finding, and not tracked by anything; delete the
  directories when convenient:
      rm -rf examples/…
```

Advisory rather than fatal: residue is local state that no clone and no CI run
has, so failing on it would make a green tree depend on a developer's history.

## It found more than the hand sweep did

The twelve `-entry` directories were found by eye. The guard's report then named
**eleven more** the sweep had missed, because they use underscores
(`native_rust_params_entry`, `native_entry`, `freertos_realtime_entry`, …) and
the glob searched for `*-entry`. All confirmed removed by phase-383 W10.a.

That is the argument for reporting rather than skipping, made by the change
itself on its first run.

## Not covered

* Whether a package that is GENERATED IN PLACE under a walked directory could be
  misread as deleted. It should not be — generated entry packages live under
  `build/<coord>/`, which the walk already prunes (phase-300 W3) — but the
  discriminator would be wrong for one that did not.
* The residue in other forms: `Cargo.lock` and `generated/` trees survive the
  same way and no gate mentions them. Only the config is checked, because only
  the config has a tracking rule.
