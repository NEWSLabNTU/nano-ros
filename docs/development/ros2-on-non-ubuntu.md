# ROS 2 on a non-Ubuntu host (Arch, Fedora, NixOS …)

nano-ros itself needs **no** ROS 2 (see
[installation.md](../../book/src/getting-started/installation.md#do-i-need-ros-2-installed)):
setup, `nros sync`, codegen and the first-node flows all work without it. ROS 2
is required only for the interop side — `ros2` CLI verification, the
`rmw_zenoh_cpp` / cyclone / fastrtps interop cells, and the ROS-bridging lanes
of `just test-all`.

The problem: ROS 2 ships apt packages for one Ubuntu LTS per edition (humble →
22.04, jazzy → 24.04) and nothing else. On Arch the AUR `ros2-humble` package
source-builds the whole tree against the rolling python/boost/OpenCV and breaks
whenever those move — not worth the time. The repo also hardcodes
`/opt/ros/<distro>/setup.bash` in `activate.sh` and in
`packages/testing/nros-tests/src/ros2.rs`, so a relocated prefix (RoboStack,
nix-ros-overlay) does not drop in either.

**The route that works: an Ubuntu distrobox sharing your home and network.**
`/opt/ros/humble` exists inside it, the checkout keeps ONE absolute path, and
every documented lane runs unchanged.

## Setup

```sh
sudo pacman -S --needed distrobox        # or dnf/apt equivalent

# podman is not required if you already use docker:
DBX_CONTAINER_MANAGER=docker distrobox create -n ros2 -i ubuntu:22.04 \
    --volume /path/to/your/checkout/parent:/path/to/your/checkout/parent -Y

# ROS 2 Humble + the packages the interop lanes use:
distrobox enter ros2 -- bash scripts/dev/ros2-distrobox-setup.sh
```

Mount the checkout at the **same absolute path** it has on the host. Different
paths on the two sides is the issue-0375 hazard: `nros sync` writes absolute
paths, and every path-keyed cache then splits in two.

`ros-humble-desktop` is ~2 GB of apt; the script also installs the book's host
prerequisites, `rmw_cyclonedds_cpp`, `rmw_fastrtps_cpp`, `rmw_zenoh_cpp`,
`domain_bridge`, `example_interfaces`, `rosidl_adapter`, colcon and rosdep, then
verifies each.

`rmw_zenoh_cpp` is what the zenoh e2e lanes run as their router (phase-362 W1).
Without it they report `[SKIPPED:capability]`, which reads as green — three
issues were closed "unverifiable" that way on 2026-08-18. `just doctor` reports
which router it can see.

## Where the box's storage lands — check `/` before you create it

`ros-humble-desktop` plus the three RMWs is roughly 3–5 GB of image layers and
apt content, and BOTH docker and rootless podman keep that under `/` by default
(`/var/lib/docker`, `~/.local/share/containers`). On a workstation whose root
filesystem is nearly full, the documented `distrobox create` above will either
fail partway through the apt step or leave the system with no headroom.

Measured on the maintainer host, 2026-08-18: `/` at 97 % (9.1 GB free) while
`/mnt/wd` — the disk holding the checkout — had 218 GB. Three ways out, in the
order worth trying:

1. **Put rootless podman's storage on the big disk.** A user-level
   `~/.config/containers/storage.conf` with `graphroot` pointing at a directory
   on that disk; no sudo, reversible by deleting the file, and `/` is untouched.
   Create the box with the default manager afterwards (drop the
   `DBX_CONTAINER_MANAGER=docker` prefix).

2. **Use docker exactly as documented** and accept the space on `/`. Simplest
   and matches the Setup section verbatim — fine when `/` has ≥ 15 GB free.

3. **Install `ros-humble-ros-base` instead of `desktop`**, keeping
   `rmw-{zenoh,cyclonedds,fastrtps}-cpp`. Roughly half the size, and a
   divergence from `scripts/dev/ros2-distrobox-setup.sh` — so when a lane later
   wants a package `desktop` would have pulled, that divergence is what you will
   be chasing. Prefer 1 or 2 unless disk is genuinely scarce.

Whichever you pick, the checkout still has to be mounted at the SAME absolute
path it has on the host (issue 0375) — see "Two paths to the same checkout".

### What a host without the box gives up

Not a nicety. On a host with no ROS install, `just ci` on 2026-08-18 reported
**176 `[SKIPPED:capability]`** tests — the whole zenoh-router-dependent surface,
including every `native_api` interop case — and they are counted as skips only
because `test-all` rewrites them. The `check-required-features-tests` lane
(issue 0652) runs a BARE `cargo nextest`, where the same skips are FAILURES, so
tier 1 cannot go green on such a host at all. The box is what turns that surface
from "unverified, quietly" into coverage.

## Build where you run — clone INSIDE the box (issue 1248)

The box is an ordinary Linux that happens to have ROS. Treat it as one:

```bash
distrobox enter ros2
git clone https://github.com/NEWSLabNTU/nano-ros      # NOT --recurse-submodules
cd nano-ros
. scripts/dev/ros2-box-env.sh      # gives this toolchain its own store
just setup-cli && just ci gate
```

**Never `--recurse-submodules`, here or anywhere** (RFC-0097 D10). The
submodules are large and most are irrelevant to any one task, and a recursive
clone also drags in `play_launch`'s layer-3 runtime submodules, which nano-ros
never builds. `scripts/bootstrap.sh` initialises the one submodule the CLI build
needs (`packages/cli/third-party/play_launch`), scoped — run it once if you use
`just setup-cli`, which builds but does not init. Everything else comes from
`nros setup --source <name>`, which fetches shallow (`--depth 1`) rather than
deepening to reach a pin.

**Never build on the host and run in the box**, and never mirror one tree into
the other. Host and box differ in compiler and libc, nothing checks that shared
artifacts agree, and the failures are loader errors naming a symbol version —
which read like code bugs (issues 0400, 0401, 0759).

`scripts/dev/ros2-box-sync.sh` used to rsync the host tree into a
`<checkout>-box` sibling, and it is **retired**. Its rules excluded build output
by directory NAME, the tree has tracked SOURCE with those names, and the two
cannot be told apart by name because provisioned source and build output share
directories. That collision ate tracked files five times — `packages/cli/
build-support/` (the box could not compile `nros` at all), Zephyr's
`drivers/i2c/target/` (could not configure any Zephyr image), then #1229, #0925
and #1243. Every one of them was a bug in the COPY, not in nano-ros. A `git
clone` has none of them.

### The one thing the box still needs from us

A distrobox **shares `$HOME` with the host by design**, so `~/.nros` and
`~/.cargo` are the same directory in both — two toolchains, one store. That is
not a box concept; it is the general rule *a store belongs to one toolchain*
meeting an environment where a different machine does not imply a different
`$HOME`. `ros2-box-env.sh` sets `NROS_HOME` and `CARGO_INSTALL_ROOT` for the
inner toolchain and does nothing else.

The build system asks exactly one question about an environment: **are ROS 2
ament packages discoverable here, so message packages can be found?** Whether
this Linux is bare metal, a container or a VM is the operator's business and is
not encoded anywhere.


## Known rough edges

- **`zstd` is not in a stock Ubuntu 22.04**, and the prebuilt dists are
  `.tar.zst`, so the first prebuilt install fails with `tar (child): zstd:
  Cannot exec`. `sudo apt-get install zstd` inside the box. → issue 0385
- **`rmw_zenoh_cpp` is an apt package on humble+** (`ros-<distro>-rmw-zenoh-cpp`;
  this note used to say humble had none). Install it like the others — there is
  no source overlay to build any more (RFC-0075, amended 2026-08-19).
- **If the box is in play, EVERY job runs in the box — against a tree cloned
  there.** Not a style preference: the two sides have different compilers and a
  different libc, and nothing in the build system checks that the toolchains
  agree. Do not half-share — a host-side `just setup-cli` or a host-side fixture
  build reaches into the same directories. Issue 0759 refused sharing one
  checkout; issue 1248 retired the rsync mirror that was the other way to get a
  second tree, because `git clone` does the same thing without a rule set that
  cannot distinguish source from build output by name.
- **A box created BEFORE that note has no router, and nothing says so loudly.**
  The setup script installs `rmw_zenoh_cpp` today; a box built earlier does not
  gain it, and the only symptom is cells reporting `[SKIPPED] zenohd failed to
  start … no rmw_zenoh_cpp/rmw_zenohd found` — a skip, so the run stays green
  (2026-08-22: every router-needing sched cell skipped here, and a measurement
  script that keys on its cell's own output line read those skips as FAILURES,
  which is the opposite misread and the reason it was noticed at all). Check
  with `just doctor`, fix with `sudo apt-get install ros-<distro>-rmw-zenoh-cpp`
  inside the box.
- **The vendored `make` and `nros-launch-resolve` are host binaries in a shared
  tree**, exactly like the CLI. The pinned make used to link the build host's
  guile and died here as `libguile-3.0.so.1: cannot open shared object file`
  (fixed: the index's source recipe configures `--without-guile`, so one binary
  serves both sides — rebuild it once in the box with
  `nros setup --tool make`).
  `nros-launch-resolve` is per-environment by design (issue 0409): a stale one
  fails `nros sync` with a TOML `unknown field` error naming a field the current
  schema does have, which reads as a broken `system.toml` rather than a stale
  binary. `just setup-launch-resolve` in the box.
- **ROS's `setup.bash` dies under `set -u`** (`AMENT_TRACE_SETUP_FILES: unbound
  variable`). Any script sourcing it needs `set +u` around the source.
- **The stale-CLI guard fires after any pull that touched CLI sources**
  (`in-tree nros CLI is STALE — its sources changed since it was built`).
  Rebuild it in the box and re-run `nros_box_publish`.
- **`source` in a pipeline runs in a subshell** — `source env.sh | tail` leaves
  your environment untouched and looks exactly like a broken activation.

## Verified on this route

`just test-unit` in the box: **817 tests, 816 passed, 2 skipped**, plus one
`[SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1`
reported as a failure by the bare-nextest quirk above. No box-specific failures.

Arch host (glibc 2.44) + Ubuntu 22.04.5 box (glibc 2.35), 2026-08-01:
`nros setup` for zenoh and cyclonedds, `nros sync`, a cyclone-backed
`examples/native/rust/talker` build, and `ros2 topic echo /chatter` receiving
its messages over `rmw_cyclonedds_cpp` with `ros2 topic list` showing
`/chatter`.
