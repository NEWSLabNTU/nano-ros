---
id: 270
title: "nros-rmw-zenoh deps zpico-sys with default features — platform-aliases TU double-defines clock symbols on orin-spe"
status: resolved
type: bug
severity: medium
area: build
---

## RESOLUTION (2026-07-28)

`nros-rmw-zenoh`'s `zpico-sys` dep is now `default-features = false`, so
`platform-aliases` + `link-ip` are no longer forced transitively (cargo can't
subtract a transitive default, which is why `platform-orin-spe` couldn't drop
them). They are re-supplied as nros-rmw-zenoh's OWN features, BOTH:

- in `default = ["platform-aliases", "link-ip"]` — for the test / native path
  that keeps default features on; and
- from each **non-orin-spe** `platform-*` feature (posix / zephyr / bare-metal /
  freertos / nuttx / threadx now forward `zpico-sys/platform-aliases` +
  `zpico-sys/link-ip`) — for the boards that build `default-features = false`.

`platform-orin-spe` forwards NEITHER (it keeps `zpico-sys/orin-spe` +
`zpico-sys/link-ivc`), so an orin-spe consumer gets `system.c`'s native `z_*`
symbols with no `platform_aliases.c` — no double-define, no post-build
ar-strip. `nros-rmw-zenoh-staticlib` gained a matching `platform-orin-spe`.

Verified via `cargo tree -e features -i zpico-sys`: under `platform-orin-spe`
the only path enabling `platform-aliases` is nros-rmw-zenoh's own
`[dev-dependencies]` (self-test build only, which wants the aliases) — never the
command-line consumer path; under `platform-posix`/`platform-freertos` the
command-line path DOES enable them (byte-identical to before). `cargo test -p
nros-rmw-zenoh` + `cargo check -p nros-board-native --features rmw-zenoh` green.
The autoware_sentinel `just build-spe-firmware` ar-strip workaround can be
dropped once it consumes this.

Note (out of scope, latent same class): `zpico-serial` also deps `zpico-sys`
without `default-features = false` — harmless for orin-spe (IVC-only, no serial)
but the same unification trap if a future serial+orin-spe graph appears.

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
