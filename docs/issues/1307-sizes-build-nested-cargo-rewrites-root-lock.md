---
id: 1307
title: "Every NuttX build rewrites the root `Cargo.lock` with a `[[patch.unused]]`
  libc block — `nros-sizes-build`'s nested cargo bypasses the `--locked` shim"
status: open
type: bug
area: build, nuttx
severity: medium
related: [issue-0359, issue-0378, issue-0464]
---

## What happens

After any NuttX build (seen 2026-09-10 and again on 2026-09-11, on the #741
rebase and on its follow-up PR #898's `tmp/libcxx-probe-a.sh` run), the root
`Cargo.lock` has a new block appended:

```toml
[[patch.unused]]
name = "libc"
version = "0.2.183"
```

CLAUDE.md says lockfiles change ONLY when a developer means it (issues
0359/0378), and `--locked` is injected project-wide by the `scripts/bin/cargo`
PATH shim (`NROS_CARGO_FLAGS`) exactly so that a lock mismatch FAILS instead of
rewriting the file. This one gets rewritten anyway, every time, so every NuttX
build leaves a dirty tracked file that someone has to notice and revert
(`git checkout -- Cargo.lock`).

## Where it comes from

`packages/tooling/nros-sizes-build/src/lib.rs`, `find_dep_rlib_isolated`.
For a NuttX target it runs a nested build to probe the executor's sizes:

- the binary is `env::var_os("CARGO")`, which inside a build script is the path
  of the REAL cargo that cargo exported, not the PATH shim. The shim `exec`s
  `"$real_cargo"`, so the nested call never passes through it and gets no
  `--locked`;
- the args are `build -p <crate> --target <nuttx triple> -Z
  build-std=std,panic_abort … --config
  'patch.crates-io.libc.path="…/third-party/nuttx/libc"'`;
- that `--config` patch is mandatory for std-from-source on NuttX (the
  crates.io libc lacks `_SC_HOST_NAME_MAX`), but the workspace the nested cargo
  resolves is the ROOT one, which does not use that libc. Cargo records the
  patch as unused, in the root lock.

The mechanism above is read from the code. The symptom is measured: the diff is
exactly the block above, and nothing else in the lock moves.

## Fix — the obvious one does not work alone

Passing `--locked` to the nested call turns the silent rewrite into a hard
failure, and `find_dep_rlib` has no fallback by design (issue 0464). So
`--locked` alone would break every NuttX build. The nested build must stop
writing the root lock at all. Options, to be measured:

1. Give the nested build its own lockfile: the NuttX lane is already nightly
   (`-Z build-std`), so `-Z unstable-options --lockfile-path <probe dir>/Cargo.lock`,
   seeded from a copy of the root lock, keeps resolution identical and puts the
   `[[patch.unused]]` write in the probe dir. Then add `--locked` against THAT
   copy, if the unused-patch entry can be pre-seeded; otherwise leave it unlocked
   and state why.
2. Apply the libc patch only where it is used: build the probe against a
   manifest/workspace that actually depends on the patched libc, so the patch is
   not unused.

Whichever is chosen, the nested call should honour `NROS_CARGO_FLAGS`, so the
project-wide rule reaches it and a future nested cargo cannot bypass it the same
way. Grep for siblings: every build script that runs `env::var_os("CARGO")`
bypasses the shim (`rg -n 'var_os\("CARGO"\)' packages`).

## Acceptance

A NuttX build (for example `NROS_CARGO_FRONTENDS=1 bash
scripts/build/fixtures-build.sh nuttx cpp zenoh`) leaves `git diff --quiet --
Cargo.lock` true, and the sizes it probes are unchanged (same `__NROS_SIZE_*`
values as before the fix).
