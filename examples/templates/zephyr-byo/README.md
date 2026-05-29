# nano-ros Zephyr starter (bring-your-own west workspace)

A **manifest + app** starter for consuming nano-ros as a Zephyr `west` module in
your own workspace — **not** a vendored or pre-patched Zephyr (Phase 205.A). It
pins a nano-ros-tested Zephyr line and imports nano-ros; the patches are applied
to *your* workspace by `nros setup` / `west patch`, never shipped pre-applied.

> Split-out target: this directory is the in-tree source for a standalone
> `NEWSLabNTU/nano-ros-zephyr-example` repo. The steps below assume it has been
> `west init`'d as its own workspace; in-tree it doubles as a copy-out reference.

## Layout

```
nano-ros-app/            # this repo (the west manifest repo)
├── west.yml             # pins Zephyr (v3.7.0) + imports nano-ros
└── app/                 # the application
    ├── CMakeLists.txt   # find_package(Zephyr) + nros_generate_interfaces()
    ├── prj.conf         # CONFIG_NROS=y + zenoh RMW
    └── src/main.c       # std_msgs/Int32 talker on /chatter
```
After `west update` the workspace gains `zephyr/` and `modules/nano-ros/`
siblings — Zephyr's standard layout.

## Quickstart (native_sim, zenoh)

```bash
# 1. Workspace
west init -m https://github.com/NEWSLabNTU/nano-ros-zephyr-example my-ws
cd my-ws && west update              # clones Zephyr + nano-ros (NOT submodules)

# 2. nano-ros CLI + per-board provisioning (toolchain/SDK/daemon + transports)
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
export PATH="$HOME/.nros/bin:$PATH"
( cd modules/nano-ros && nros setup zephyr --rmw zenoh )   # zenoh-pico + mbedtls + zenohd
( cd modules/nano-ros && nros setup --source px4-rs )      # workspace cargo-load dep

# 3. Patches into YOUR workspace (Zephyr 4.x: `west patch apply` instead)
for p in nsos-recvmsg-patch native-sim-ipproto-ip-patch nsos-adapt-ipproto-ip-patch; do
    bash modules/nano-ros/scripts/zephyr/$p.sh "$PWD"
done

# 4. Build + run
west build -b native_sim/native/64 app
zenohd -l tcp/127.0.0.1:7456 &       # the router (from ~/.nros store, or build/zenohd)
./build/zephyr/zephyr.exe            # → "Published: 1", "Published: 2", …
```

A real board (e.g. `qemu_cortex_a9`) swaps `-b` and the SDK targets; see the book.

## Notes

- **Tested Zephyr pin.** `west.yml` pins `v3.7.0` (LTS). The patches + sha/anchors
  are keyed to nano-ros-tested versions; bump `zephyr` revision in lockstep with a
  nano-ros release. (4.4 line: `v4.4.0`, Python ≥ 3.12, `-S nros-zenoh` snippet.)
- **Rust apps** name their `[lib]` `rustapp` (a `zephyr-lang-rust` contract) and
  generate their crate-patch config with
  `nros generate-rust --generate-config --nano-ros-path <ws>/modules/nano-ros/packages/core`.
- Full reference: the book's *Getting Started → Zephyr (west module)*
  (`book/src/getting-started/integration-zephyr.md`) and
  `modules/nano-ros/examples/zephyr/` for the multi-RMW examples.
- Verified end-to-end (Phase 202): this flow builds `app` to `zephyr.exe` and runs
  to `Published: 1` on a fresh BYO workspace.
