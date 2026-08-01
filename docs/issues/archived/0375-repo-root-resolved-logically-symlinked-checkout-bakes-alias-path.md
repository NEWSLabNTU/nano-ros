---
id: 375
title: "Repo root is resolved logically, so a checkout reached through a symlink bakes the alias path into NROS_REPO_DIR / nano_ros_ROOT / the rc line"
status: resolved
type: tech-debt
area: build
related: [rfc-0048, issue-0363, issue-0372, phase-218]
resolved_in: "3de28c939"
---

# Repo root is resolved logically, not physically

`activate.sh:20-26`, `activate.fish:12` and `scripts/bootstrap.sh:6-7` all
derived the workspace root with the `cd "$(dirname …)" && pwd` idiom, which
returns the **logical** path — symlinks intact. A checkout reached through a
symlinked parent therefore recorded the alias everywhere, and one tree acquired
two names.

Observed on a host where `/home/aeon/data` symlinks to `/mnt/wd`:

```
$ echo $PWD                     # logical
/home/aeon/data/data/projects/nano-ros
$ pwd -P                        # physical
/mnt/wd/data/projects/nano-ros
$ source ./activate.sh; echo $NROS_REPO_DIR
/home/aeon/data/data/projects/nano-ros
$ ./scripts/bootstrap.sh        # proposed rc line
source "/home/aeon/data/data/projects/nano-ros/activate.sh"
```

Blast radius (latent — no failure was reproduced): RFC-0048's `nros sync`
writes **absolute** paths into the central `nros-patch.toml` and leaf
`.cargo/config.toml` includes, so entering the tree by the other spelling later
hands cargo different absolute paths for the same crates (issue 0363 shows how
badly a mismatched sync degrades); and cargo fingerprints, `CONFIGURE_DEPENDS`
globs, sccache keys and fixture-freshness probes all key on paths.

## Resolution

`3de28c939`. All three sites resolve physically — `cd -P` / `pwd -P` in the two
shell files, `pwd -P` in the fish mirror. Verified by entering through the
alias: `NROS_REPO_DIR`, `nano_ros_ROOT` and the rc line bootstrap proposes all
come out `/mnt/wd/...`.

**Not covered:** paths derived from the *shell's cwd* rather than from the
activation files — `justfile_directory()` and anything downstream of it still
report whatever spelling the user `cd`'d in with. Fixing that means normalizing
at the justfile entry point, which is a separate change.
