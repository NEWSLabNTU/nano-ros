---
id: 375
title: "Repo root is resolved logically, so a checkout reached through a symlink bakes the alias path into NROS_REPO_DIR / nano_ros_ROOT / the rc line"
status: open
type: tech-debt
area: build
related: [rfc-0048, issue-0363, issue-0372, phase-218]
---

# Repo root is resolved logically, not physically

## Summary

`scripts/bootstrap.sh` and `activate.sh` both derive the workspace root with the
`cd "$(dirname …)" && pwd` idiom, which returns the **logical** path — the one
built from `$PWD`, symlinks intact — not the physical one. A user whose checkout
is reachable through a symlinked parent therefore gets the alias path recorded
everywhere, and the same tree acquires two names.

Observed on this host, where `/home/aeon/data` is a symlink to `/mnt/wd`:

```
$ echo $PWD        # logical
/home/aeon/data/data/projects/nano-ros
$ pwd -P           # physical
/mnt/wd/data/projects/nano-ros

$ source ./activate.sh; echo $NROS_REPO_DIR
/home/aeon/data/data/projects/nano-ros
```

`bootstrap.sh` then proposes an rc line carrying the alias too:

```
bootstrap: proposed addition to /home/aeon/.zshrc:
source "/home/aeon/data/data/projects/nano-ros/activate.sh"
```

Sites: `activate.sh:20-26` (`_nros_root`, exported as both `NROS_REPO_DIR` and
`nano_ros_ROOT`), `scripts/bootstrap.sh:6-7` (`SCRIPT_DIR` / `REPO_ROOT`) — two
spellings of the same idiom, so a fix belongs in both, not in whichever one is
reported.

## Why it matters

No failure was observed during this setup run — filing it as a latent hazard with
a concrete blast radius, not as a reproduced break:

- **RFC-0048 `nros sync` writes ABSOLUTE paths** into the central
  `nros-patch.toml` and leaf `.cargo/config.toml` includes. Whichever path
  spelling was live when sync ran is the one baked in; entering the tree by the
  other spelling later gives cargo a different absolute path for the same
  crates. Issue 0363 already shows how badly a partial/mismatched sync degrades.
- **Two spellings defeat build caches**: cargo fingerprints, `CONFIGURE_DEPENDS`
  globs, sccache keys and the fixture-freshness probes all key on paths, so the
  same tree entered two ways can rebuild or, worse, read a stale artifact as
  fresh.
- The repo already treats this alias class as a known hazard elsewhere: the data
  archive's own notes record that pointing a tool at `/home/aeon/data` instead of
  the `/mnt/wd` realpath made git-annex silently skip staging.

## Direction

Resolve physically at both sites — `cd -P "$(dirname …)" && pwd` (or
`realpath`/`readlink -f` where available, with the `cd -P` fallback for portability)
— and have the rc line, `NROS_REPO_DIR`, `nano_ros_ROOT` and the bootstrap-printed
next steps all carry the physical path. Worth a one-line note in `nros doctor`
comparing `$NROS_REPO_DIR` against its realpath, so an already-provisioned host
that baked the alias is told rather than left to debug a cache mystery.
