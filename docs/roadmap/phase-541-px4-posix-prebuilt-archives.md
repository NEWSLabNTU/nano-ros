# Phase 541 — The release ships the PX4 POSIX archives prebuilt

**Status (2026-11-06). IN PROGRESS — W1 to W3 done in their pull request; W4
waits on that pull request merging.** Implements
[RFC-0097 D13](../design/0097-release-composition-and-version-axes.md) and the
"Installed toolchain" decision in
[RFC-0039](../design/0039-px4-integration-architecture.md). Closes
[issue 2289](../issues/2289-px4-release-ships-no-posix-archives.md).

Decided (maintainer, 2026-11-06): option A of issue 2289. The release ships the
PX4 POSIX build inputs prebuilt, so a user-tier PX4 build has no contributor
step. Option B, a user verb that builds them (`nros px4 deps`), was rejected: it
makes every PX4 user build nano-ros's C++ runtime and puts a Rust toolchain on a
firmware host.

## Why

`integrations/px4/NanoRosPx4Module.cmake` links two archives and compiles
against the config headers the same cargo run wrote. A released SDK root is a
`git archive`, so it had none of them, and PX4's configure stopped at
`FATAL_ERROR` "required nano-ros archive not found". The remedy named a
`cargo build -p nros-cpp` inside the SDK root, which the book cannot tell a
reader to run (RFC-0117, RFC-0124 Principle 5). The book's PX4 page
(`guide/platforms/px4`) could not be replayed from a release, and its probe
track `px4` carries an `awaits` naming issue 2289.

## What ships

Measured by reading the module and building once (x86_64-unknown-linux-gnu,
rustc 1.99.0, at this phase's commit):

| Path under `share/nano-ros/prebuilt/px4-posix/` | What | Size |
|---|---|---|
| `lib/libnros_cpp.a` | `nros-cpp`, features `alloc,env,log,panic-platform,rmw-cffi,platform-posix`, no networked backend | 14,954,910 B |
| `lib/libnros_platform_posix.a` | `packages/platform/nros-platform-posix` | 56,198 B |
| `placed/nros-c-generated/rclc/detail/nros_config_generated.h` | C sizes, variant `nros_config_variant_sz_*` | |
| `placed/nros-cpp-generated/rclcpp/detail/nros_cpp_config_generated.h` | C++ config, variant `nros_cpp_config_variant_*` | |
| `nros-px4-prebuilt.toml` | `nano_ros` commit (copied from the manifest), target, features, rustc | |

Total 15,059,986 B uncompressed, 4,081,842 B as zstd at `tar --zstd`'s
default level, which is what the asset grows by. The local replay's prefix
(no runner packages staged) was 31,176,099 B compressed with it.

Everything else the module reads is source the SDK root already ships: the four
include dirs (`packages/api/nros-{c,cpp}/include`, `packages/core/nros-rmw-abi/include`,
`packages/platform/nros-platform-api/include`), `integrations/px4/include`, the
uORB backend's sources and `cmake/NanoRosLocate.cmake`.

## Work items

### W1 — Build and stage them in the release — DONE

`scripts/stage-px4-posix-archives.sh <prefix>` builds the archive under the
header placer and the platform shim with cmake, copies both into the prefix,
writes the stamp, and verifies before anything ships: each placed header's
variant symbol is defined by the archive (the module's pairing rule), and the
archive defines no `nros_rmw_{zenoh,xrce,cyclonedds}_register`. Paths come from
`nros locate artifact`, the layout the module itself asks. The feature list is
read from the module's one `set(_NROS_PX4_UORB_FEATURES …)` line.

`release-nros.yml` runs it in `Stage the prefix`, after `stage-sdk-root.sh` and
`nros toolchain stage-packages` (the manifest is written first, so the stamp
can copy its commit), and `Prove the asset installs` checks the installed prefix
holds both archives and a stamp naming the manifest's commit.
`scripts/probe/build-release-asset.sh` installs `cmake` in its runner shim,
because the hosted image has it and a bare `ubuntu:22.04` does not.


### W2 — The module finds them in an installed toolchain — DONE

`NanoRosPx4Module.cmake` decides by the root: a checkout (it has `.git`) keeps
today's defaults (`nros locate artifact cpp-archive`, `build/nros-px4/placed`,
`build/nros-platform-posix`); an SDK root reads `prebuilt/px4-posix/`. There is
no precedence between the two, so a prebuilt dir staged by hand in a checkout
never shadows the tree being edited. Explicit `-D…`/env overrides beat both, as
before. An SDK root with no prebuilt dir (a toolchain released before this
phase, or a source export) still fails at configure, now naming both ways out
instead of a cargo command against the SDK root.

Version and ABI: the stamp's `nano_ros` must equal the manifest's
(`<sdk-root>/../nros/manifest.toml`), or configure refuses naming both commits;
the header/archive pairing check runs as before. A networked `BACKENDS` against
the shipped uORB-only archive is refused at configure, because the llvm-nm
precheck is skipped on a host with no Rust.

### W3 — Probe — DONE (what ran is stated exactly)

Ran on this host (2026-11-06), against a prefix staged by the release's own
commands: `nros toolchain manifest`, `scripts/stage-sdk-root.sh`,
`scripts/stage-px4-posix-archives.sh`, and `nros` + `nros-launch-resolve`
copied to `<prefix>/bin`. Then, under `env -i` (no `NROS_*` variables, no
`activate.sh`, no checkout on `PATH`):

1. `nros dev sdk root` answered the prefix's `share/nano-ros`;
2. the module template was copied out of it, and PX4
   (`third-party/px4/PX4-Autopilot` at the commit the superproject pins, used
   read-only, built out of tree) was configured `px4_sitl_default` with
   `EXTERNAL_MODULES_LOCATION` set to the copy. Configure printed `PX4 POSIX
   archives: prebuilt` and both pairing lines;
3. the build linked `bin/px4` (1123 steps), compiling the uORB backend from
   the prefix's sources;
4. `bin/px4 -i 7 -d etc/init.d-posix/rcS` booted headless ("Startup script
   returned successfully"), and `px4-nano_ros_app --instance 7 start` printed
   `nano-ros uORB backend registered`, rc 0.

The three other outcomes, configure only: a checkout (the worktree, after the
module header's two build commands) configures as before; a stamp naming
another commit is refused naming both commits; an SDK root with no prebuilt
dir is refused naming both ways out.

The host's Python lacked `attrs` for PX4's `jsonschema`, so PX4's own
`Tools/setup/requirements.txt` went into a venv first; that is PX4's
`ubuntu.sh` step, not a nano-ros one.

Not run: the book's `px4` probe track in its container (the book session runs
it, W4), `just probe installed` (the asset build in `ubuntu:22.04`), and
`release-nros.yml` itself, which runs on dispatch only. The workflow's new
lines are the commands above; `Prove the asset installs`' new check was not
executed (it needs the installed layout `install.sh` produces).

### W4 — Book: drop the `awaits` — WAITING on this phase's pull request

Owned by the book session: once this merges, the `px4` track in
`packages/boards/board-support.toml` drops its `awaits` on issue 2289 and
replays `guide/platforms/px4.md` on the installed track (SITL `make`, headless
boot, `px4-nano_ros_app start`). Issue 2289 closes when that replay is green.

## Acceptance

- A released toolchain's PX4 SITL configure needs no cargo step and no checkout.
- A checkout's PX4 build is unchanged.
- The release refuses to stage archives whose headers do not pair, or that carry
  a networked backend; the module refuses a prebuilt dir from another toolchain.
