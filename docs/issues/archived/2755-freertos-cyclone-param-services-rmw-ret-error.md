---
id: 2755
title: "On mps3-an536-freertos with Cyclone DDS a C++ node's parameter services fail at describe_parameters with `rmw_ret error`"
status: resolved
type: bug
severity: medium
area: [rmw, params, freertos]
related: [2704, 1268]
found: 2026-11-19
resolved: 2026-11-20
resolved_in: "fix(cyclonedds, issue 2755): enum QoS fields are stored at the ABI's enum size"
---

## Measured

nano-ros 93368ca929, the downstream island's controller image
(`controller_bringup:an536`, board `mps3-an536-freertos`, `rmw =
"cyclonedds"`, `features = ["param_services"]`, 164 parameters, one node
`/controller`), branch `nano-ros-next`. `nros run controller_bringup:an536`
on QEMU:

```
[INFO] nros: [    9.002000] parameter store: 164 slots (1389408 B) + state and services (61400 B) carved from the executor backing
...
[INFO] nros: [    9.084000] Actuation Safety Island is Live
[ERROR] nros: [    9.107000] parameter services could not be created for node /controller at service describe_parameters: Transport(Backend("rmw_ret error")). `ros2 param` cannot reach 1 of this image's 1 node(s) ...
[ERROR] nros: [    9.107000] nros: NodeError::Transport(Backend("rmw_ret error")): transport error: backend error: rmw_ret error
```

The image runs on without its parameter services. The same bringup on
`freertos-posix` (host Cyclone) creates them. Unlike issue 2704 (Zephyr,
`Transport(Unsupported)`, no descriptors) the failure is a backend error at
the second service, so `get_parameters` was created.

## Cause

Not a resource limit, and not specific to `describe_parameters`.

- **Intermittent.** Re-measured on QEMU with the same image: about 1 boot in
  8 failed, at a different service each time (`describe_parameters`,
  `set_parameters_atomically`, `set_parameters`). After the first failure
  every retry failed too.
- **Where.** `dds_create_reader`/`dds_create_writer` returned
  `DDS_RETCODE_BAD_PARAMETER` from `ddsi_xqos_valid`, which named a
  different enum policy as invalid each time (DURABILITY, DESTINATION_ORDER,
  PRESENTATION, CYCLONE_IGNORELOCAL, HISTORY).
- **The values were valid.** A copy of the merged QoS taken at the failure
  held durability 0, destination order 0, ignorelocal 0. Cyclone's default
  QoS tables and its pid descriptor tables matched the ELF byte for byte, no
  task's stack had overflowed, and a replay of every heap event showed no
  overlapping allocation.
- **The read was 4 bytes.** Cyclone's generic plist code
  (`ddsi_plist.c`: validate, (de)serialise, compare, duplicate, free, print)
  handles every XE1/XE2/XE3 field as an `unsigned`. arm-none-eabi's default
  is variable-size enums (`Tag_ABI_enum_size: small`), so in this image each
  policy kind is 1 byte (`ptype /o struct dds_qos`: `durability` at 64, size
  1, then a 7-byte hole). Validation read the kind plus the padding after it,
  and the padding held whatever the recycled heap block held before. That is
  the intermittency, and why a dirty block reused for the next attempt kept
  failing.
- **Deterministic consequences on the same targets.** PRESENTATION is the
  kind plus two flags, so a coherent or ordered flag was read as part of the
  kind and the following flags were read 4 bytes in. Deserialising a remote
  endpoint's QoS wrote 4 bytes into each 1-byte field.

Every 32-bit arm-none-eabi Cyclone image is affected (mps3-an536,
s32z270). The Zephyr FVP image (AArch64) and hosted builds have int-sized
enums.

## Fix

In the Cyclone fork (branch `nano-ros`, `05b3c9ed`): the XE fields are
stored through an enum type declared for the purpose, which takes the policy
kinds' size under either ABI, and `ddsi_plist_init_tables` asserts that size
for each kind type. The wire stays 4 bytes, and the toolchain's enum size is
unchanged.

## Verification

The controller image above (`controller_bringup:an536`, QEMU):

| build | boots | parameter services failed |
| --- | --- | --- |
| before (93368ca929's Cyclone) | about 69 | 9 |
| Cyclone and its rmw built with `-fno-short-enums` (diagnosis only) | 25 | 0 |
| the fix, enums left short | 50 | 0 |
