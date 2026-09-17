---
id: 1307
title: "Every NuttX build rewrites the root `Cargo.lock` with a `[[patch.unused]]`
  libc block — `nros-sizes-build`'s nested cargo bypasses the `--locked` shim"
status: resolved
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

## Resolution

Fixed 2026-09-18. Option 1 (a lockfile of the probe's own), because option 2
turned out to be answering a question the measurement had already settled —
see "why not option 2" below.

### Reproduced first, through the real code path

The probe was driven by a throwaway harness calling the real
`nros_sizes_build::find_dep_rlib` with the environment a `nros-c` build script
sees on `armv7a-nuttx-eabihf` (the FFI bundle's own dep-site feature list,
`RUSTUP_TOOLCHAIN=nightly-2026-04-11`, cwd = the consumer's manifest dir). One
run, and the root lock moved:

```
--- root lock md5 before: 888e917d1a46a1ffd3e8ad6ba1bcf4e8
--- root lock md5 after:  efb7ff6625b35f98d74476aa0e488e59
```

```diff
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -4164,3 +4164,7 @@ dependencies = [
  "zpico-link-ivc",
  "zpico-platform-custom",
 ]
+
+[[patch.unused]]
+name = "libc"
+version = "0.2.183"
```

Exactly the reported block and nothing else, and the probe answered 18 sizes
(`EXECUTOR_SIZE 88992`).

### The mechanism is sharper than "the root workspace does not use that libc"

The root lock pins `libc 0.2.186` from the registry; the patched copy is
`0.2.183`. Those versions cannot meet, so cargo says so itself —
`help: If the patch has a different version from what is locked in the
Cargo.lock file, run cargo update to use the new version` — and records the
unused patch in the lock it was given. Worth stating because it explains a
surprise: the patch has NO effect on the probe's resolution at all (the `nros`
rlib is built against the crates.io libc, which on the pinned nightly compiles
std's NuttX port fine), yet it is the whole cause of the write.

`--locked` alone was measured to do exactly what the issue predicted:

```
error: cannot update the lock file /…/Cargo.lock because --locked was passed to prevent this
```

### The fix

`packages/tooling/nros-sizes-build/src/lib.rs`:

* `apply_nested_lock_discipline()` — when the invocation injects a `[patch]`
  the workspace lock cannot record (today: the NuttX libc), the nested cargo
  resolves against a COPY in its own probe target dir:
  `--config resolver.lockfile-path="<probe dir>/probe-lockfile/Cargo.lock"`,
  seeded byte-for-byte from the workspace lock by `seed_probe_lockfile()` so
  resolution — and therefore every `__NROS_SIZE_*` — is unchanged. The seed is
  kept beside the copy and compared by CONTENT, never mtime (every rebase and
  `git stash` moves that).
* It honours `NROS_CARGO_FLAGS` when the variable is SET (unset means an
  out-of-tree consumer, whose lock is not ours to have an opinion about):
  every non-lock flag is forwarded verbatim, `--locked`/`--frozen` are
  forwarded on invocations that resolve the workspace lock as-is, and are
  dropped only on the redirecting one — `--frozen` leaving its `--offline`
  half behind. That is the same distinction the `scripts/bin/cargo` shim
  already draws: it skips `--locked` when the target's lock is not a tracked
  file, and the redirected copy is a build artifact.
* Which flag switches the redirect on is version-dependent, and measured, not
  assumed:

  | cargo | `--config resolver.lockfile-path` |
  | --- | --- |
  | 1.96.0-nightly (the pinned NuttX nightly) | `warning: ignoring resolver.lockfile-path, pass -Zlockfile-path to enable it`, and the lock was written anyway |
  | 1.97.0-nightly | honoured; `-Z lockfile-path` now warns "has been stabilized in the 1.97 release" |
  | 1.98.1 stable | honoured, and any `-Z` is a hard error |

  So `-Z lockfile-path` is passed only below minor 97, read from
  `$CARGO --version` (`cargo_minor`) rather than from `rustc`, which corrosion
  can point elsewhere.
* `WorkspaceLockGuard` — the backstop, deliberately NOT the mechanism. A
  redirect cargo silently ignores is indistinguishable from one that worked,
  which is how this class comes back; the guard snapshots the workspace lock
  around the nested build, restores it byte-for-byte if it moved, and warns
  that the redirect did not take effect. It restores rather than fails: the
  size the probe reports is correct either way, and the dirty tracked file is
  the part that is not.

### Why not option 2 (build the probe where the patch is used)

The patch is unused because a `0.2.183` path patch cannot satisfy a lock that
pins `0.2.186`, not because the probe's workspace is the wrong one. Making it
used would mean either moving the root lock (the thing this issue exists to
stop) or resolving the probe in the FFI crate's workspace, which changes what
`-p nros` means and so risks changing the ANSWER — the number that sizes a C
caller's buffer (issue 0464).

### Acceptance, measured

Same harness, same coordinate, a FRESH probe dir (so the nested cargo
re-resolves from scratch):

```
--- root lock md5 before: 888e917d1a46a1ffd3e8ad6ba1bcf4e8
harness rc=0
--- root lock md5 after:  888e917d1a46a1ffd3e8ad6ba1bcf4e8
$ git diff --quiet -- Cargo.lock ; echo $?
0
```

and the 18 probed sizes are byte-identical before and after
(`diff` of the two dumps is empty; `EXECUTOR_SIZE 88992` both times). The
`[[patch.unused]]` block is now in
`<probe dir>/probe-lockfile/Cargo.lock` — 99252 bytes against the 99200-byte
seed — and no `WorkspaceLockGuard` warning fired, so the redirect did the work
rather than the backstop.

The non-redirecting arm was measured too, because forwarding `--locked` to it
is new: a host-target probe (`x86_64-unknown-linux-gnu`, cargo 1.98.1 stable,
`NROS_CARGO_FLAGS=--locked`) answers `rc=0` with its 18 sizes and a clean root
lock. And the forwarding itself is proven wired rather than assumed:
`NROS_CARGO_FLAGS="--locked --nros-1307-wiring-probe"` makes the nested cargo
fail with `unexpected argument '--nros-1307-wiring-probe' found`.

### The sweep

```
rg -n 'var_os\("CARGO"\)|var\("CARGO"\)' packages
```

Six shim-bypassing cargo invocations exist. What each subcommand does to the
workspace lock was MEASURED from `packages/api/nros-c`, with the same unused
`[patch]` that makes `build` write it:

| invocation | subcommand | lock |
| --- | --- | --- |
| `nros-sizes-build` `find_dep_rlib_isolated` | `build` | was DIRTY — fixed |
| `nros-sizes-build` `resolved_features_for` | `metadata --no-deps` | CLEAN |
| `nros-sizes-build` `cargo_target_dir` | `metadata --no-deps` | CLEAN |
| `nros-sizes-build` `workspace_lockfile` (new) | `locate-project --workspace` | CLEAN |
| `nros-sizes-build` `cargo_minor` (new) | `--version` | CLEAN |
| `nros-cli-core` `warn_if_cargo_predates_config_include` | `--version` | CLEAN |
| *(negative control)* | `metadata` WITH deps | **DIRTY** |

The control matters: without it "`metadata` is safe" would be a claim about a
flag nobody tested. `metadata` counts as non-resolving only together with
`--no-deps`.

`Command::new("cargo")` sites (~10, in tests and the CLI) are NOT in this
class — that spelling resolves through PATH, so the shim already reaches them.

### Gate

`just check nested-cargo-lock-discipline`
(`scripts/check/check-nested-cargo-lock-discipline.py`), fast lane, source-only.
A function that runs cargo through a shim-bypassing program must either confine
itself to a measured non-resolving subcommand or carry lock discipline
(`--locked`/`--frozen`, or `resolver.lockfile-path`).

Negative control on the real tree: against `origin/main`'s
`nros-sizes-build/src/lib.rs` it reports `find_dep_rlib_isolated … runs
['build'] with no lock discipline` and exits 1; against the fixed file, 0
unaccounted. `--list` prints the sweep table above so the next person does not
re-derive it by hand.

The brace masking in it is not incidental polish: a naive brace count ran
through `ws.rs`'s `extract_cargo_path_deps` (which carries `{` in a comment and
in a `"{names:?}"` format string) and made that function absorb its neighbour
200 lines away — a body that absorbs its neighbours also absorbs their
`--locked`, which is a gate reporting OK over a violation. The self-test plants
exactly that shape.
