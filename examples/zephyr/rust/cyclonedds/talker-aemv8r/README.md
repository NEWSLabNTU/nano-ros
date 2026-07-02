# zephyr/rust/cyclonedds/talker-aemv8r

Phase 217.D.1 — nano-ros **Rust** pub/sub demo on the ARM FVP
`Base_RevC AEMv8-R` Cortex-A SMP target under Zephyr 3.7. Rust-side
sibling of `examples/zephyr/cpp/cyclonedds/talker-aemv8r/`; same
`std_msgs/String` payload on `/chatter` so a single FVP run + peer
listener exercises both languages.

Carve-out per CLAUDE.md "Examples = Standalone Projects" — this
example is pinned to one board (`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`)
and one RMW (Cyclone DDS), mirroring the cpp sibling. The
board-agnostic `examples/zephyr/rust/talker/` keeps the multi-RMW Cargo
feature surface.

## Build

```sh
just zephyr setup       # one-time: workspace + SDK + zephyr-lang-rust
just zephyr build-fvp-aemv8r-cyclonedds-rust
```

Or manually:

```sh
cd zephyr-workspace
west build \
    -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp \
    -d build-fvp-aemv8r-cyclonedds-rust-talker \
    nano-ros/examples/zephyr/rust/cyclonedds/talker-aemv8r
```

Board glue (BOARD / per-board prj.conf / DTS overlay / default RMW /
runner) flows from `nano_ros_use_board(fvp-aemv8r-smp)` (Phase 215.B
contract). The example's own `prj.conf` is bare; the Cyclone-DDS knobs
live in `prj-cyclonedds.conf` for shape-parity with the board-agnostic
`examples/zephyr/rust/talker/`.

## Runtime

ARM FVP `Base_RevC AEMv8R` is license-gated (Arm Development Studio /
[Arm Ecosystem FVPs](https://developer.arm.com/downloads/-/arm-ecosystem-fvps)).
After accepting the EULA + installing locally, point nano-ros at it:

```sh
export ARM_FVP_DIR=/path/to/Base_RevC_AEMv8R   # or ARMFVP_BIN_PATH=<dir>
just zephyr run-fvp-aemv8r-cyclonedds-rust
```

Phase 214.A / 215.D.4 — the recipe resolves `FVP_BaseR_AEMv8R` via
`scripts/zephyr/resolve-fvp-bin.sh` and shells `west fvp run` which
drives Zephyr's `cmake/emu/armfvp.cmake` with the canonical
`board.cmake` `-C` flags (UART 0–3 → stdout, GICv3, NUM_CORES from
`CONFIG_MP_MAX_NUM_CPUS`).

Pair with:

- A second `native_sim` instance running
  `examples/zephyr/rust/listener` (or the cpp sibling) for an in-tree
  round-trip.
- Stock ROS 2 `ros2 topic echo /chatter std_msgs/msg/String` for
  cross-stack interop.

## Wire backend

Cyclone DDS C++ RMW from `packages/dds/nros-rmw-cyclonedds/`, pulled in
via the board crate's `default_rmw = "cyclonedds"` + this example's
`prj-cyclonedds.conf` (`CONFIG_NROS_RMW_CYCLONEDDS=y` + `CONFIG_CPP=y`).
The wire format on this binary is byte-equal with stock
`rmw_cyclonedds_cpp` peers — same contract as Phase 117 / 175.A.

## Shape

- `src/lib.rs` — Component pkg (lib only). `nros::node!(Talker)` exports
  `register(runtime)` for the generated runtime to drive (Phase 212.M.3).
- `Cargo.toml` — standalone workspace root (Phase 208.F1); single
  Cargo feature `rmw-cyclonedds` (default).
- `CMakeLists.txt` — `nano_ros_use_board(fvp-aemv8r-smp)` BEFORE
  `find_package(Zephyr)`; standard `rust_cargo_application()` build.
- `prj.conf` + `prj-cyclonedds.conf` — base + RMW overlay (mirrors the
  board-agnostic Rust talker convention).
- `boards/` — none. The FVP per-board overlay is supplied by the board
  crate, not duplicated here.
