---
id: 2448
title: "An owned entry with no `nros` dependency gets no adapter, so a board-only test program cannot become an owned-entry workspace"
status: open
type: bug
severity: medium
area: [cli, orchestration, testing]
related: [2406, 1305]
found: 2026-11-11
---

**Home: phase-531 W7 step-3 prep** (the seven board-only bins below cannot
leave the leaf `<pkg>_nros_selection` shape until this is fixed).

## Measured

`bins/condvar-contract-threadx-linux`, migrated to the owned-entry shape
`bins/message-info-observer` uses (RFC-0071 D19): `src/<pkg>` keeps its
hand-written `main`, depends on `nros-board-threadx-linux` and
`<pkg>_nros_derived = { path = "../../generated/nros-derived/<pkg>" }`, and
the bringup's `[image.threadx-linux]` names `entry = "<pkg>"`.

`nros build condvar_contract_threadx_linux_bringup:threadx-linux` fails:

```
error: failed to load manifest for dependency `condvar_contract_threadx_linux_nros_derived`
  failed to read `.../generated/nros-derived/condvar-contract-threadx-linux/Cargo.toml`
```

The adapter is never written. `orchestration::facade::write_facade_in`
returns `Ok(None)` when the entry's manifest has no `nros` path dependency
(`deps.nros.is_none()`), and says nothing. The adapter's render reads that
path for the edition and capability axes (`AdapterSite::render`,
`deps.nros.clone().expect("checked by the caller")`).

## Who it blocks

Seven of the remaining W7 bins are board-only programs. They reach the board
crate (`run_bare`, `ThreadxLinux`, ...) and `nros-log`, but not `nros`:

- `condvar-contract-freertos-mps2`, `condvar-contract-threadx-linux`
- `logging-smoke-freertos-mps2`, `logging-smoke-nuttx-qemu-arm`,
  `logging-smoke-threadx-linux`, `logging-smoke-threadx-riscv64`
- `pool-exhaustion-threadx-linux`

Their leaf shape got the board's image features (RMW, `[board_config]`
netstack, ...) through the selection package, which re-exports the board.
An owned entry gets them through its adapter, and without the `nros`
dependency there is none.

## What would close it

An owned entry whose manifest names a board crate but not `nros` gets an
adapter that carries the board features its image implies, and leaves the
`nros` axes out. Or `nros build` refuses that shape with a sentence naming
the remedy, instead of a missing-manifest error from cargo. Then the seven
bins migrate and their QEMU/host cells pass on the workspace rows.

## Found with

The census pre-pass refused the same image first: `entity-census take
--plan` resolved a SystemModel that an owned-entry bringup with no
`[[component]]` row never has (configure generates no launch for it,
`ws::synthesise_bringup_launch`). That half is fixed with the W7 RTOS slice
(`entity_census::owned_entry_without_model`, asked by both census sites).
