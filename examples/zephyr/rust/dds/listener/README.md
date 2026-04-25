# Zephyr DDS Listener Example (Phase 71.8)

ROS 2 / DDS-RTPS subscriber running on Zephyr RTOS via `nros-rmw-dds`
(dust-dds, pure-Rust, no_std + alloc). Counterpart to the
`dds/talker` example.

## Build (native_sim)

```bash
west build -b native_sim/native/64 \
    nros/examples/zephyr/rust/dds/listener
./build/zephyr/zephyr.exe
```

## Domain ID

Set via `CONFIG_NROS_DDS_DOMAIN_ID` in `prj.conf` (default `0`). Must
match the talker's domain id for discovery.
