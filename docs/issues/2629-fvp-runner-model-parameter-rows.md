---
id: 2629
title: "The fvp runner has no row for FVP model parameters, so a Zephyr image on fvp-aemv8r-smp needs ARMFVP_EXTRA_FLAGS for its network, tap and UART file"
status: open
type: enhancement
area: [boards, emulator, zephyr, runner]
severity: medium
found: 2026-11-16
related: [phase-524, rfc-0115]
---

GitHub: NEWSLabNTU/nano-ros#4007

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md)**, W3 (registry
rows and board descriptors). The FVP is `fvp-aemv8r-smp`'s twin.

## What happens

A Zephyr image on `fvp-aemv8r-smp` needs FVP model parameters, and no board
or image row can state them. The Autoware safety island passes them through
`ARMFVP_EXTRA_FLAGS`, the env that Zephyr's `cmake/emu/armfvp.cmake` reads at
configure time (its `build.sh`, lines 411–424):

- `-C bp.smsc_91c111.enabled=1 -C bp.smsc_91c111.promiscuous=1`. Without these
  the model has no network at all, so they are a fact about the board.
- `-C bp.hostbridge.userNetworking=0 -C bp.hostbridge.interfaceName=<tap>`, for
  tap networking. This is a choice per image.
- `-C bp.pl011_uart1.out_file=<file>`, the trace UART to a file. Also per image.

Its `run_zephyr_model()` (build.sh:339) then parses `build.ninja` for the
`run_armfvp` command line, because `nros run` cannot boot the same model.

There is nowhere to say any of this:

- `[board.run] emulator = "fvp:FVP_BaseR_AEMv8R"` names the model and nothing
  else.
- `packages/runners/nros_runner_fvp` reads no model parameters.
- `[image.<id>.run] nic` takes `slirp`, `{ bridge = … }` or `{ tap = … }`, and
  only QEMU reads it (`book/src/reference/system-toml.md`).

## Expected

- A board row for the fixed model parameters. On `fvp-aemv8r-smp` that is the
  SMSC NIC enable.
- Image run rows for what an image chooses: tap networking (the existing
  `nic = { tap = … }`, honoured by the fvp runner as well) and a UART output
  file.

`nros run <image>` then boots the same model the consumer boots today, with no
`ARMFVP_EXTRA_FLAGS` and no parsing of `build.ninja`.

## Notes

- Whatever spelling is chosen has to reach both roads that start the model:
  `nros run` (the runner) and Zephyr's own `west build -t run` (armfvp.cmake
  reading `ARMFVP_EXTRA_FLAGS`). If the image's `west build` and `nros run`
  boot different models, the twin is no longer the image's.
- Consumer code: https://github.com/NEWSLabNTU/autoware-safety-island/blob/nano-ros/build.sh#L411-L424
