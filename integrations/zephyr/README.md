# nano-ros — Zephyr integration

> **Canonical BYO guide:** the step-by-step user quickstart lives in the book at
> `book/src/getting-started/integration-zephyr.md` (rendered: *Getting Started →
> Zephyr (west module)*). It is the single source of truth for the BYO workflow —
> prerequisites (`nros setup zephyr`), `prj.conf`, build/run, RMW selection, and
> patches. **This file is the dir-level reference** for the integration shell
> itself: the `west.yml` import fragment and the patch-delivery mechanics
> (`west patch` index, the rust/cyclonedds script fallbacks). Where the two
> overlap, the book is authoritative; keep this file's procedural steps in sync
> with it (Phase 202.6).

This directory is the Zephyr **integration shell** for nano-ros. A
downstream "bring-your-own" (BYO) Zephyr workspace imports it so that
`west update` pulls nano-ros and Zephyr discovers it as a module
(`module.yml` + `CMakeLists.txt` + `Kconfig`).

## Adding nano-ros to your workspace

In your workspace's `west.yml`:

```yaml
manifest:
  remotes:
    - name: nano-ros
      url-base: https://github.com/NEWSLabNTU
  projects:
    - name: nano-ros
      remote: nano-ros
      path: modules/nano-ros
      import:
        file: integrations/zephyr/west.yml
```

Then `west update`. Enable the module in your `prj.conf` with
`CONFIG_NROS=y` (and pick an RMW via `CONFIG_NROS_RMW`), and link your
app against `NanoRos::NanoRos` (done automatically by this shell).

## Prerequisites + transport sources

> **Canonical procedure: the book** (`integration-zephyr.md` → *Prerequisites*).
> This is a short pointer so the steps don't fork — see the book for the
> authoritative version.

Install the `nros` CLI, then run **`nros setup zephyr --rmw <rmw>`** once. That
one command provisions the cross-toolchain/SDK bits, the RMW host daemon
(`zenohd` / the XRCE agent), **and the RMW transport submodules** — zenoh-pico +
mbedtls for `zenoh`, the cyclonedds fork for `cyclonedds` (`west update` clones
nano-ros but **not** its transport submodules, so this step is required). In a
BYO workspace run it from the nano-ros checkout: `cd modules/nano-ros && nros
setup zephyr --rmw zenoh`.

One more source: the nano-ros cargo build loads the whole workspace, which
path-deps `px4-sitl-tests`, so also provision **px4-rs** (a small source, not a
PX4 build): `cd modules/nano-ros && nros setup --source px4-rs`. Without it the
`nros-c` cargo build fails `failed to get px4-sitl-tests`.

(For a single transport, `nros setup --source <name>` also works; the west-native
alternative is `submodules: true` on the `nano-ros` project, but it pulls *all*
submodules incl. unrelated platform SDKs.)

## Applying nano-ros patches in your workspace

nano-ros needs a few small patches to Zephyr's Native Simulator
Offloaded Sockets (NSOS) for `native_sim` networking (these implement
`recvmsg()` and IPv4-multicast `setsockopt`/`getsockopt` forwarding —
without them Cyclone DDS receive busy-spins and SPDP multicast discovery
never works). These are delivered to your workspace via Zephyr's
`west patch` mechanism.

`west patch` reads the index that ships with nano-ros at
`<manifest-repo>/zephyr/patches.yml` (`<manifest-repo>` = the `nano-ros`
project), with the patch files under `<manifest-repo>/zephyr/patches/`.

From your workspace, **after `west update`**:

```sh
west patch list     # show the patch entries (parses patches.yml)
west patch apply    # apply all patches, verifying each sha256
# ... build, flash, run ...
west patch clean    # roll back to the manifest checkout (git checkout .)
```

`west patch apply` re-runs idempotently only against a clean checkout; if
a later `west update` rewinds the module, re-apply. Use
`west patch apply --roll-back` to auto-revert partially-applied patches
on failure.

### Zephyr 4.x only

`west patch` ships with Zephyr 4.x (`scripts/west_commands/patch.py`). It
is **absent on Zephyr 3.7 LTS** — 3.7 users instead run nano-ros's own
sed/python patch scripts directly:

```sh
scripts/zephyr/nsos-recvmsg-patch.sh           <workspace-dir>
scripts/zephyr/native-sim-ipproto-ip-patch.sh  <workspace-dir>
scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh  <workspace-dir>
```

(The `*-4.4.sh` variants are the 4.x ports that `west patch` wraps; the
script path stays the canonical mechanism for nano-ros's own in-tree
build, which builds against both Zephyr lines. The `west patch` index is
purely an **additive** delivery path for downstream 4.x workspaces.)

### Rust examples need additional patches (not in `west patch`)

If you build the **Rust** examples (not C/C++), nano-ros patches
`zephyr-lang-rust` (and, per board, Zephyr's Rust Kconfig) — these are **not**
in `patches.yml` (they edit the `modules/lang/rust` project + are board/arch-
conditional, so a static `west patch` index is the wrong tool). Run the scripts
from the nano-ros checkout against your workspace, after `west update`:

```sh
# All Rust examples (multi-RMW feature forwarding into cargo):
modules/nano-ros/scripts/zephyr/cargo-features-patch.sh        <workspace-dir>
modules/nano-ros/scripts/zephyr/rust-cargo-extra-args-patch.sh <workspace-dir>
# Per target arch (only if you build Rust for these):
modules/nano-ros/scripts/zephyr/cortex-a9-rust-patch.sh        <workspace-dir>  # qemu_cortex_a9 (Zynq)
modules/nano-ros/scripts/zephyr/aarch64-rust-patch.sh          <workspace-dir>  # ARMv8-R AArch64 (fvp_aemv8r)
modules/nano-ros/scripts/zephyr/cortex-r-rust-patch.sh         <workspace-dir>  # Cortex-R52 (S32Z)
```

All are idempotent + version-tolerant (warn-and-skip if the upstream shape moved,
so a `zephyr-lang-rust` bump doesn't hard-fail). The **C and C++** examples need
none of these. (nano-ros's own in-tree `just zephyr setup` runs the applicable
set automatically; BYO workspaces invoke them directly.)

### Cyclone DDS patches are NOT delivered via `west patch`

nano-ros also carries five Cyclone-DDS-on-Zephyr patches (thread TLS,
log flush, sockwaitset self-pipe, UDP rcvbuf, best-effort multicast
join). These are **not** listed in `patches.yml` because Cyclone DDS is
consumed through the nano-ros submodule
(`third-party/dds/cyclonedds`), pinned at a commit that already has them
baked in — there is no upstream-pulled Cyclone tree for `west patch` to
modify. If you vendor Cyclone DDS some other way and target Zephyr, apply
`scripts/zephyr/cyclonedds-zephyr-*.sh` against your Cyclone checkout
yourself.

## Upstreamability

The three NSOS patches are generic Zephyr fixes (a missing `recvmsg`
implementation and IPv4-multicast forwarding for the native simulator),
not nano-ros-specific, and are flagged `upstreamable: true` in
`patches.yml`. Opening Zephyr PRs for them is a human follow-up — this
phase does not open any PRs. The Cyclone-on-Zephyr patches are
downstream-only.
