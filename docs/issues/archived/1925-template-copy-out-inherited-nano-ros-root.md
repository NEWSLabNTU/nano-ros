---
id: 1925
title: "`check-template-copy-out` and `check-scaffold-builds` configure a copied-out CMake project with no `-Dnano_ros_ROOT`: a clean shell cannot find nano_ros, a worktree shell builds the parent checkout"
status: resolved
type: bug
area: [build, testing, cmake]
severity: medium
found: 2026-10-16
related: [1280, 1840, 1108]
---

## Measured

Both gates copy a project OUT of the checkout (a template, or an `nros new`
scaffold in `$TMPDIR`) and then run a bare

```
cmake -S <copy> -B <copy>/build [-DCMAKE_TOOLCHAIN_FILE=…]
```

with no `-Dnano_ros_ROOT`. A copy outside the checkout has no walk-up, so
`find_package(nano_ros)` has exactly one input left: whatever `nano_ros_ROOT`
the CALLER's environment holds. Since RFC-0103 D6 (amended 2026-10-12)
`activate.sh` exports none, and since issue 1840 it UNSETS one that names
another checkout. Measured in a linked worktree
(`/mnt/mx500/aeon/nros-worktrees/ptco`, origin/main `b7e63a5b44`), invoking the
scripts directly after `source ./activate.sh`:

**(a) clean env** (`env -u nano_ros_ROOT`):

```
$ scripts/check-template-copy-out.sh cpp-port-minimal-publisher
  cpp-port-minimal-publisher/mps2-an385-freertos: FAIL — the copy does not build
      CMake Error at CMakeLists.txt:13 (find_package):
        … Could not find a package configuration file provided by "nano_ros" …
check-template-copy-out: FAILED — a template a user copies does not build.

$ scripts/check-scaffold-builds.sh c_project       # same class
  CMake Error at CMakeLists.txt:9 (find_package): … "nano_ros" …
  CMakeCache.txt: nano_ros_DIR:PATH=nano_ros_DIR-NOTFOUND
```

**(b) `nano_ros_ROOT=<main>` inherited** (the main checkout —
what a worktree spawned from a pre-D6 shell holds; a `cmake` shim recorded the
cache after each configure):

```
check-template-copy-out (mps2-an385-freertos copy):
  nano_ros_DIR:PATH=<main>
  -- nano-ros: Corrosion 0.6.1 via SDK store … (asked by <main>)
  CMake Error at <main>/cmake/NanoRosRmwDispatch.cmake:255 …
  <main>/packages/api/nros-c/CMakeLists.txt:60 (nros_feature_set)
  (the main checkout's cmake driving the worktree's `nros`:
   "unrecognized subcommand 'entity-facts'")

check-scaffold-builds c_project:
  nano_ros_DIR:PATH=<main>
  CMake Error at <main>/cmake/bootstrap.cmake:97 …
```

So the gate measured the PARENT checkout's CMake tree — issue 1280's class: an
inherited absolute path outranks the checkout being built.

## Why `just check …` hid it

`just/sdk-env.just` exports `nano_ros_ROOT := _NROS_HERE` to every recipe, so
`just check template-copy-out` and `just check scaffold-builds` happened to get
THIS checkout's root in both shapes (measured: `just --evaluate nano_ros_ROOT`
prints the worktree with the variable unset AND with it inherited from main).
The scripts themselves carry a `Usage:` line and are run directly to narrow to
one template, and on that road the correctness of the gate depended on an
implicit just-only export it never named. The road a user is told to take
(book `first-node-c.md`, `freertos.md`) passes `-Dnano_ros_ROOT=…` explicitly;
the gates that exist to reproduce that road did not.

## Fix direction

One helper both gates configure through, which names THIS checkout's root
explicitly (RFC-0103 D6 rung 1, `-Dnano_ros_ROOT`) and drops the inherited
`nano_ros_ROOT`/`NROS_REPO_DIR` from the configure's environment, plus a gate
that refuses a bare `cmake` configure in a `scripts/check-*.sh` gate.

## Resolution

One helper, one gate (commit `fix(#1925): copy-out gates configure against
THIS checkout's root`).

1. **`scripts/build/copy-out-cmake.sh` — `nros_copy_out_cmake_configure`.**
   Runs `cmake <args…> -Dnano_ros_ROOT=<this checkout>` with `nano_ros_ROOT`
   and `NROS_REPO_DIR` dropped from its environment. "This checkout" is the
   one the helper lives in, found by `checkout-paths.sh`'s marker walk
   (`nros_checkout_root`) — never an environment variable, so there is nothing
   inherited to re-root. `-Dnano_ros_ROOT` is RFC-0103 D6's explicit rung and
   the spelling the book gives a user configuring a copied-out leaf.
   `check-template-copy-out`'s cross arm and `check-scaffold-builds`'s CMake
   road both configure through it.

2. **Gate `just check copy-out-cmake-root`** (fast lane, no build):
   runs the helper against a stub `cmake` with `nano_ros_ROOT` and
   `NROS_REPO_DIR` inherited from a foreign path — the stub's argv must name
   `-Dnano_ros_ROOT=<this checkout>` and its environment must hold neither —
   and refuses a bare `cmake` configure (`-S`/`-B` on the line) in any
   `scripts/check-*.sh` / `scripts/check/*.sh`; `# copy-out-root-ok: <reason>`
   exempts a configure of something that is not a nano-ros consumer. Its
   `--self-test` feeds the source rule two bare configures (flagged) and a
   helper call, a `--build`, a comment and an exempt line (passed).

Sweep: `git grep -n nano_ros_ROOT` and every `cmake -S/-B` configure under
`scripts/ just/ tests/ .github/ packages/testing/`. Only the two gates above
configure a copied-out nano-ros consumer; `tests/*.sh` export
`nano_ros_ROOT="$PROJECT_ROOT"` or `include()` modules by path, and in-tree
builds walk up (`cmake-incremental.sh`, re-rooted since issue 1840).

Measured after the fix, in the same worktree:

| Shape | `check-template-copy-out cpp-port-minimal-publisher` | `check-scaffold-builds c_project cpp_project` |
|---|---|---|
| clean (`env -u nano_ros_ROOT`), direct | OK, `nano_ros_DIR:PATH=<worktree>` | OK, `nano_ros_DIR:PATH=<worktree>` |
| `nano_ros_ROOT=<main>` inherited, direct | OK, `nano_ros_DIR:PATH=<worktree>` | OK, `nano_ros_DIR:PATH=<worktree>` |

and `env -u nano_ros_ROOT just check template-copy-out` (all templates): OK,
9 projects built, 5 Zephyr leaves NOT VERIFIED (issue 1782, unchanged).

Mutation proof of the gate (each reverted, `cmp`-restored, gate green again):

| Mutation | Gate |
|---|---|
| template-copy-out cross arm back to bare `cmake` | FAIL (source rule) |
| scaffold-builds back to bare `cmake -S` | FAIL (source rule) |
| helper drops `-Dnano_ros_ROOT=$root` | FAIL (configure does not name this checkout) |
| helper drops `env -u nano_ros_ROOT -u NROS_REPO_DIR` | FAIL (inherited root reached the env) |
| source rule stops ignoring comments | FAIL (self-test) |

Not fixed here, seen while measuring: with a long `TMPDIR` the copy-out build
fails in `scripts/build/mirror-generated-header.sh` with `File name too long`
(its `nros-mirror-ran/` stamp name is the whole output path with `/` → `_`).
The default `/tmp` stays under the limit.
