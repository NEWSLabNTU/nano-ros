# Phase 32: Platform/Transport Architecture Split

**Status: Complete**

## Summary

Refactored the monolithic BSP crates into separate platform crates (system primitives + user API) and link crates (network protocol implementations).

## Completed Work

| Step | Description | Status |
|------|-------------|--------|
| 32.1 | Add `link-*` Cargo features to `nano-ros-transport-zenoh-sys` | Complete |
| 32.2 | Create `nano-ros-link-smoltcp` crate | Complete |
| 32.3 | Create `nano-ros-platform-qemu` crate | Complete |
| 32.4 | Decouple C shim layer | Complete |
| 32.5 | Migrate `nano-ros-bsp-qemu` to wrapper, then migrate examples | Complete |
| 32.6 | Migrate ESP32-C3 BSPs | Complete |
| 32.7 | Migrate STM32F4 BSP | Complete |
| 32.8 | Update feature flag chain (`shim-*` → `platform-*`) | Complete |
| 32.9 | Move link crate to `packages/link/` | Complete |
| 32.10 | Rename zenoh shim crates (`zenoh-pico-shim` → `nano-ros-transport-zenoh`) | Complete |
| — | Delete 4 BSP wrapper crates | Complete |
| — | Migrate all QEMU examples to `nano-ros-platform-qemu` directly | Complete |
| — | Archive completed phase docs and superseded design docs | Complete |

## Result

```
packages/
├── core/               # Core library crates
├── transport/          # Zenoh transport middleware
│   ├── nano-ros-transport-zenoh/
│   └── nano-ros-transport-zenoh-sys/
├── link/               # Link protocol crates (bare-metal)
│   └── nano-ros-link-smoltcp/
├── platform/           # Platform crates (system primitives + user API)
│   ├── nano-ros-platform-qemu/
│   ├── nano-ros-platform-esp32/
│   ├── nano-ros-platform-esp32-qemu/
│   └── nano-ros-platform-stm32f4/
├── bsp/                # Only Zephyr remains
│   └── nano-ros-bsp-zephyr/
├── drivers/            # Hardware drivers
│   ├── lan9118-smoltcp/
│   └── openeth-smoltcp/
└── ...
```

## Superseded Items

The following Phase 32 items are superseded by [Phase 33](phase-33-crate-rename.md) which implements the broader rename and RMW abstraction:

- 32.11 (docs update) — docs archived; remaining updates happen as part of Phase 33
- 32.12 (tidy) — BSP deletion done; remaining cleanup (delete `c/platform_smoltcp/`) folded into Phase 33

## Future Work (moved to Phase 33+)

- Crate rename: `nano-ros-*` → `nros-*` / `zpico-*` — see Phase 33 (complete)
- Platform crate split: `nano-ros-platform-*` → `zpico-platform-*` + `nros-*` — see Phase 33.3 (complete)
- RMW trait abstraction — see `docs/design/rmw-layer-design.md`
