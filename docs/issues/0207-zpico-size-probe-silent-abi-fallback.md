---
id: 207
title: "zpico size_probe failure warn-and-continues with guessed SOCKET_SIZE/ENDPOINT_SIZE — silent pass-by-value ABI skew on cross targets"
status: open
type: bug
severity: medium
area: zpico
related: [issue-0135]
---

## Problem (audit 2026-07-16, I3)

`packages/zpico/nros-zpico-build/src/runner.rs:956-980`: when the
`size_probe` compile fails, the build warns and continues with hardcoded
`SOCKET_SIZE=16` / `ENDPOINT_SIZE=8`. The code's own comment calls it a
known foot-gun: a wrong `_z_sys_net_socket_t` size silently skews the
pass-by-value FFI ABI and shows up later as runtime `ConnectionFailed` on
cross targets (the 0135 mismatched-TU class, reintroduced dynamically).

## Fix sketch

Hard-fail when the target is embedded/cross (real struct size unknowable
from the host); the host-native fallback may keep the warning path if the
sizes are actually derivable there.
