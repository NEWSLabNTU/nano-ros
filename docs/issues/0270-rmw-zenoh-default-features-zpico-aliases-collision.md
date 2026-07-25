---
id: 270
title: "nros-rmw-zenoh deps zpico-sys with default features — platform-aliases TU double-defines clock symbols on orin-spe"
status: open
type: bug
severity: medium
area: build
---

## Finding (autoware_sentinel phase-14 SPE build, 2026-07-25)

`nros-rmw-zenoh/Cargo.toml` line ~99:

```toml
zpico-sys = { version = "0.5.0", path = "../zpico-sys" }
```

Default features ON → `platform-aliases` + `link-ip`. zpico-sys's own
`orin-spe` feature documents that the alias TU must be OFF there ("the
SPE's system.c implements the `_z_*` surface natively via the FSP
V10.4.3 FreeRTOS API, so `platform-aliases` is OFF for orin-spe to
avoid double-define") — but cargo feature-unification cannot subtract:
any consumer linking `nros-rmw-zenoh` + `zpico-sys/orin-spe` gets BOTH
`system.c` and `platform_aliases.c`, and the spe.elf link dies:

```
multiple definition of `z_time_elapsed_ms';  (system.o vs platform_aliases.o)
multiple definition of `z_time_elapsed_s'
multiple definition of `_z_get_time_since_epoch'
```

## Workaround shipped in autoware_sentinel

`just build-spe-firmware` ar-strips `*platform_aliases.o` from the
staticlib after cargo (safe: every symbol the alias TU forwards exists
natively in the orin-spe system.c).

## Fix direction

`nros-rmw-zenoh` should declare
`zpico-sys = { default-features = false, ... }` and re-add `link-ip` /
`platform-aliases` only from its own platform-* features (posix et al),
mirroring how the `platform-posix` feature already forwards
`zpico-sys/posix`.
