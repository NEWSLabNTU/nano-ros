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
prerequisites, `rmw_cyclonedds_cpp`, `rmw_fastrtps_cpp`, `domain_bridge`,
`example_interfaces`, `rosidl_adapter`, colcon and rosdep, then verifies each.

## Using it

```sh
distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; <command>'
```

`ros2-box-env.sh` sources `activate.sh` and adds the box-local overrides.

One-time per box. Every cargo-INSTALLED tool has to be reinstalled here: the
host's `~/.cargo/bin` copies are host-built and die with `GLIBC_2.xx not found`,
which surfaces as `just: … not found` or `no such command: nextest` rather than
as anything mentioning glibc.

```sh
cargo install just --locked
cargo install cargo-nextest --locked   # just test-unit / test / test-all drive it
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
nros_box_publish                       # defined by ros2-box-env.sh
nros setup native --rmw zenoh          # the box needs its OWN SDK store
nros setup native --rmw cyclonedds     # for the DDS interop cells
```

Then the normal tiers, all through the same entry form:

```sh
distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; just build-test-fixtures'
distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; just test-unit'
distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; just test-all'
```

The test harness reads `build/zenohd/zenohd`, not the SDK store — `just zenohd
setup` populates it (`test-all` depends on `build-zenohd` already).

## Why the overrides exist — glibc direction

glibc is **backward** compatible: a binary linked against the box's older glibc
runs on a newer host, never the reverse. So box-built artifacts work on both
sides, and host-built ones are unusable in the box. That single fact explains
every override:

| Override | Without it |
| --- | --- |
| `CARGO_TARGET_DIR` | cargo re-runs the cached build-script **executables**; a host-built `build-script-build` dies with `GLIBC_2.xx not found`. Not churn — a hard failure. |
| `NROS_HOME` | a shared store reports the host's zenohd "present" at the pinned version, then hands the box a binary it cannot exec |
| `CARGO_INSTALL_ROOT` | `~/.cargo/bin/just` is host-built and fails the same way — and a box-built copy written back to that path would break the host in turn |

Shared safely: `~/.rustup` (toolchains target an old glibc) and the cargo
registry/git caches (sources, not objects).

`CARGO_TARGET_DIR` hides the CLI from `activate.sh`'s PATH entry and from
cmake's `find_program` HINTS, both of which look at
`packages/cli/target/release/nros` — hence `nros_box_publish`, which copies the
box build there. **A host-side CLI rebuild overwrites it and breaks the box
until you re-publish.**

## Two paths to the same checkout

distrobox mounts the whole host filesystem at `/run/host` **and** translates the
entry cwd into it, so the checkout is reachable inside the box as both
`/run/host/mnt/…` and the bind-mounted `/mnt/…`, depending on how you entered.
Both are the same tree; only one matches the host's absolute path. A box build
under one and a host build under the other silently disagree — `nros sync`
writes absolute paths and the cargo/cmake caches key on them (the issue-0375
hazard). `ros2-box-env.sh` strips the `/run/host` prefix when the stripped path
is the same checkout, so `NROS_REPO_DIR` matches the host either way.

## Known rough edges

- **`zstd` is not in a stock Ubuntu 22.04**, and the prebuilt dists are
  `.tar.zst`, so the first prebuilt install fails with `tar (child): zstd:
  Cannot exec`. `sudo apt-get install zstd` inside the box. → issue 0385
- **`rmw_zenoh_cpp` has no humble apt package** (it starts at iron). Cyclone and
  FastRTPS interop work out of the box; zenoh interop needs the pinned source
  overlay (`just rmw_zenoh setup`).
- **ROS's `setup.bash` dies under `set -u`** (`AMENT_TRACE_SETUP_FILES: unbound
  variable`). Any script sourcing it needs `set +u` around the source.
- **The stale-CLI guard fires after any pull that touched CLI sources**
  (`in-tree nros CLI is STALE — its sources changed since it was built`).
  Rebuild it in the box and re-run `nros_box_publish`.
- **`source` in a pipeline runs in a subshell** — `source env.sh | tail` leaves
  your environment untouched and looks exactly like a broken activation.

## Verified on this route

Arch host (glibc 2.44) + Ubuntu 22.04.5 box (glibc 2.35), 2026-08-01:
`nros setup` for zenoh and cyclonedds, `nros sync`, a cyclone-backed
`examples/native/rust/talker` build, and `ros2 topic echo /chatter` receiving
its messages over `rmw_cyclonedds_cpp` with `ros2 topic list` showing
`/chatter`.
