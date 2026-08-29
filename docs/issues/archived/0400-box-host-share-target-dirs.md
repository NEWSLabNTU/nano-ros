---
id: 400
title: Recipes that hard-set a relative target dir escape the distrobox's
  CARGO_TARGET_DIR, so host and box share build-script binaries
status: resolved
type: bug
area: build
related: [0375, 0383, 0399]
---

## Problem

`scripts/dev/ros2-box-env.sh` redirects `CARGO_TARGET_DIR` to
`$HOME/.cargo-target-box` for one stated reason: cargo re-runs cached
build-script EXECUTABLES, and a host-built one cannot run in the box —

```
build-script-build: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
```

Recipes that set their own target dir escape that override. `check-workspace-embedded`
does `CARGO_TARGET_DIR=target-embedded cargo clippy …` (justfile:2264) — a
RELATIVE path, resolved against the shared checkout, so host and box write the
same tree. A host `just ci` followed by a box `just ci` dies in
`check-workspace-embedded` on a build script the host compiled, naming
`nros-rmw-cffi` — a crate that has nothing to do with the failure.

The recipe prints a hint for a DIFFERENT cause at that point ("a NEW host-only
member is leaking `std`… declare the new crate host-only"), which is what makes
this expensive: the hint is confident, unrelated, and points at whatever crate
was added most recently.

Same shape for the other explicit dirs: `target-zenoh` (4 sites),
`target-zenoh-fixture-posix`, `target-xrce`, `target-tls`,
`target-ros-edition-<distro>-<rmw>`.

### Second instance, FIXED (2026-08-03)

`scripts/build/link-determinism-fixture.sh` built with cargo (honouring
`CARGO_TARGET_DIR`) but copied from a HARDCODED `$repo_root/target/debug/libnros_c.a`.
In the box that copies the HOST's archive — built by some other lane with
different features — so `build/link-determinism/libnros_c.a` contained no
`nros_rmw_zenoh_register` at all and `staticlib_duplicate_symbols` failed with

    `-u nros_rmw_zenoh_register` did not pull the backend register entry into the image

i.e. a link-MODEL error message for what was a stale file from another machine
image. Fixed by resolving the archive from `${CARGO_TARGET_DIR:-$repo_root/target}`
and failing loudly when it is absent. The justfile recipes above are unfixed.

Worth sweeping for the rest of the class: any script or recipe that pairs a
`cargo build` with a hand-built `target/…` path.

### Third instance, and this one has no one-sided fix

`nros-launch-resolve` builds into its own `packages/cli/nros-launch-resolve/target/`,
outside the redirect, and `nros sync` invokes it by absolute path (issue 0285).
A host build links `libpython3.14.so` (Arch); in the box that is

    error while loading shared libraries: libpython3.14.so.1.0: cannot open shared object file

and a box build links `libpython3.10.so`, which the HOST then cannot load.
Unlike the CLI — where glibc's backward compatibility makes the box build
usable on both sides, which is why `nros_box_publish` works — the Python
soname is not compatible in either direction, so ONE binary cannot serve both.
It needs either a per-side path (a target dir that honours CARGO_TARGET_DIR,
as everything else here) or abi3 linkage.

Today the loser is whoever ran second: `just build-test-fixtures lane=native`
dies in `generate-bindings` mid-sync.

#### CORRECTION (2026-08-29): the abi3 direction below DOES NOT WORK

Checked against PyO3's documentation while fixing the same coupling elsewhere:
**abi3 is an extension-module mechanism and does not apply to embedding.** For
an embedded interpreter the ABI is fixed by the `libpython` linked at build
time, not by `Py_LIMITED_API`; there is no supported link against a
version-agnostic `libpython3.so`; and upstream's answer for "one artifact across
CPython versions with an embedded interpreter" is to stop embedding and become a
cdylib extension module.

The caveat this section lists — "confirm neither pyo3's embedding path nor the
launch parser's Python usage needs a non-limited symbol" — is moot, because the
premise fails one level above it.

The real direction is **`dlopen` at runtime**, tracked in
[issue 0897](../0897-resolver-libpython-runtime-discovery.md). Read that instead
of the section below, which is kept because a recommendation that looked settled
for months is worth leaving visible next to its refutation.

#### Recommended direction: abi3 (stable-ABI) linkage, not a per-side path

The per-side-path option (route the resolver's target dir through
`CARGO_TARGET_DIR`) only makes host and box each build+keep their OWN resolver.
That works but doubles a slow pyo3 build and leaves a resolver that still cannot
be COPIED between machines — every consumer needs the matching interpreter.

Prefer **abi3**: build the resolver's pyo3 against the CPython *stable ABI* so ONE
binary loads whatever `libpython3` the running side provides, instead of pinning
a minor-version soname (`libpython3.14.so.1.0` vs `libpython3.10.so.1.0`). The
resolver embeds CPython via `pyo3` with `auto-initialize`
(`.../ros-launch-resolve/resolve/Cargo.toml`, pyo3 0.24); adding the `abi3-py3N`
feature (floor = the lowest interpreter any target box ships, e.g. 3.10) makes
the compiled binary limited-API and version-agnostic across CPython ≥ floor —
the same one-artifact-serves-both property glibc already gives the CLI.

Caveats to verify before committing:

* the pyo3 dep lives in the **play_launch repo** (submodule), so enabling abi3 is
  an upstream/coordinated change, not a nano-ros-only edit;
* abi3 restricts pyo3 to the limited API — confirm neither pyo3's embedding path
  (`auto-initialize`) nor the launch parser's Python usage needs a
  non-limited symbol at the chosen floor;
* embedding still needs SOME `libpython3` present at runtime on each side; abi3
  removes the *soname/minor-version* coupling, not the runtime dependency.

If abi3 proves infeasible for the embedding path, fall back to the per-side
target dir (route `nros-launch-resolve`'s build through `CARGO_TARGET_DIR`).

## Repro

```sh
just ci                                              # host (glibc 2.44)
DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- \
    bash -c '. scripts/dev/ros2-box-env.sh && just ci'   # box (glibc 2.35)
```

Second run fails in `check-workspace-embedded`. `rm -rf target-embedded` clears
it until the next host run.

## Fix sketch

Make the dedicated dirs nest under the active `CARGO_TARGET_DIR` when one is
set, so the box gets its own copy and the host is unchanged:

```sh
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/}target-embedded"
```

Unset (host) → `target-embedded`, exactly as today. Set (box) →
`$HOME/.cargo-target-box/target-embedded`. Apply to every explicit target dir,
not just the one that surfaced (`grep -n 'target-dir\|CARGO_TARGET_DIR=' justfile just/*.just`).

Worth checking whether the `check-workspace-embedded` hint can distinguish the
two causes — a `GLIBC_.* not found` in the build-script output is unambiguous
and should print a different remedy.

### Ephemeral scratch dirs FIXED (2026-08-03)

The reported failure (`check-workspace-embedded`, `target-embedded`) and its
exact sibling (`test-zpico-multisession`, `target-zpico-multisession`) are the
sub-class that is safely fixable ONE-SIDEDLY: pure clippy/test scratch dirs that
no downstream reader opens at a fixed relative path. Both now root their suffix
at the active base via a shared helper `nros_scoped_target_dir <suffix>`
(`scripts/build/cargo.sh`): `${CARGO_TARGET_DIR:-$PWD/target}-<suffix>`. Host →
`$PWD/target-<suffix>` (byte-identical to the old relative dir — recipes run at
repo root); box → `$HOME/.cargo-target-box-<suffix>`, box-private and outside
the shared checkout, so host and box never reuse each other's build-script
binaries. `clean` removes both forms (it is non-shebang and cannot source the
helper, so the same expansion is inlined, with a pointer to the helper). Chose
the SIBLING spelling over the fix-sketch's nested `$CARGO_TARGET_DIR/target-embedded`
so scratch clippy objects never mix into the box's main target tree, and the
host path is unchanged. Verified: recipe exits 0 into `$repo/target-embedded` on
host; with `CARGO_TARGET_DIR` set it resolves to `<base>-embedded`.

### What REMAINS — the coupled sub-class (not one-sided fixable)

The other explicit dirs are consumed at their relative path by a downstream
reader, so making the writer box-aware without moving the reader would break the
consumer — the same "no one-sided fix" shape as the third instance:

* `target-zenoh` — `just _run` execs `target-zenoh/release/<bin>` right after
  building (threadx-linux.just); freertos/threadx-riscv64 fixtures read it too;
* `target-xrce` — px4 fixture binaries consumed under `.../target-xrce/`;
* `target-ros-edition-<distro>-<rmw>` — the ros-editions fixture reader expects
  `examples/native/rust/*/target-ros-edition-<distro>-<rmw>/debug/`;
* `target-zenoh-fixture-posix` — `scripts/build/fixture-inventory.py` names it as
  a `build_root` / `shared_mutation`.

Each needs a COORDINATED writer+reader change (or to route through
`fixtures-target-dir.sh`, which already emits repo-root-absolute dirs — those in
turn are shared host/box via the checkout, a related but distinct gap). Plus the
third instance (the `nros-launch-resolve` python soname) still needs a per-side
path or abi3. Left open for those.

## Notes

Found while running tier 1 in the box for the issue-0383 `-Werror` work
(2026-08-03). Not caused by that change; it predates it and reproduces on a
clean tree.

### This issue was filed as 0398 first — `git log` still says so

`just issue-new` could not reach origin (auth), so it fell back to
local-max+1 and picked an id another session had already reserved and pushed
(#398, component/node name decoupling). Renumbered to 400 in `2f6f7b9ba`, with
`refs/issue-ids/0400` properly reserved.

Three commits landed before the renumber and their SUBJECTS still say 0398.
They belong to THIS issue:

    73ca1b0e2  docs(0398): box needs python3-tomli; file the shared-target-dir collision
    223622478  fix(0398): link-determinism fixture must copy from CARGO_TARGET_DIR
    d4e1abc0a  docs(0398): third instance — the launch resolver cannot serve both sides

They are not amended because they are pushed to shared `main`, which several
sessions track; rewriting those subjects would mean a force-push that
invalidates every fetched copy — a worse trade than three misleading subjects
with this mapping recorded next to them. `git log --grep=0398` finds both this
issue's commits and the other one's, so read the mapping above before assuming
which is meant.

## RESOLVED (2026-08-04) — the box got its own tree

Both issues were the same premise: host and box sharing one checkout whose build
artifacts are glibc- and toolchain-specific. Fixing instances did not converge —
five in one session (build scripts, CMake caches, the CLI, the resolver, fixture
paths), each real, each a symptom.

`scripts/dev/ros2-box-sync.sh` mirrors the working tree (uncommitted edits
included, `.git` included, every build output excluded) into `<checkout>-box`,
and `ros2-box-env.sh` detects the `.nros-box-tree` marker and does NOT redirect
`CARGO_TARGET_DIR` there. Cargo then writes to the LEAF paths the fixture
contract names, inside a tree the host never touches — which is what 0401 said
redirection could never give: the two mechanisms are mutually exclusive, and the
tree split removes the conflict instead of trading one hole for another.

A mirror rather than `git worktree`, deliberately: a worktree cannot check out
the branch the host has, and carries only COMMITTED state — the loop here is
edit, build in the box, test, and a worktree would test the last commit.

Verified: box `just setup-cli` produced a working box CLI and left the host's
binary untouched (previously each overwrote the other); a fixture built in the
box landed at `examples/native/rust/talker/target/nros-fast-release/talker` —
the exact path the test-side resolver stats — inside the box tree.

A checkout WITHOUT the marker is still treated as shared and keeps the redirect:
there the alternative is host-built build scripts dying on glibc, so the old
behaviour remains correct for that case.

Caveat, documented in the script and the guide: `nros sync` writes absolute
paths into leaf `.cargo/config.toml` files, so a mirrored leaf still points at
the source tree until re-synced in the box. Same rule as any moved checkout.

The narrower guards from this session stay as defence in depth: the CMake
compiler-version cache guard, `nros_scoped_target_dir` for ephemeral dirs, the
per-side resolver path, and the SDK store honouring `NROS_HOME`.
