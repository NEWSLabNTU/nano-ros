# Zephyr DDS Talker Example (Phase 71.8)

ROS 2 / DDS-RTPS publisher running on Zephyr RTOS via `nros-rmw-dds`
(dust-dds, pure-Rust, no_std + alloc). Uses the cooperative
`NrosPlatformRuntime<ZephyrPlatform>` driven by `Executor::spin_once()`
— no OS threads, no router, no agent.

## Build (native_sim)

```bash
west build -b native_sim/native/64 \
    nros/examples/zephyr/rust/dds/talker
./build/zephyr/zephyr.exe
```

## Build (qemu_cortex_m3)

```bash
west build -b qemu_cortex_m3 \
    nros/examples/zephyr/rust/dds/talker
west build -t run
```

## Domain ID

Set via `CONFIG_NROS_DDS_DOMAIN_ID` in `prj.conf` (default `0`). RTPS
PSM ports are derived as `7400 + 250·domain` (PB+DG); peers must use
the same domain id to discover each other.
