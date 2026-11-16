---
id: 2629
title: "The fvp runner has no row for FVP model parameters, so a Zephyr image on fvp-aemv8r-smp needs ARMFVP_EXTRA_FLAGS for its network, tap and UART file"
status: resolved
type: enhancement
area: [boards, emulator, zephyr, runner]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "phase-524 W3"
related: [phase-524, rfc-0115]
---

GitHub: NEWSLabNTU/nano-ros#4007

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)**, W3 (registry
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

## Resolution

The design is RFC-0118 D4a, with a pointer in RFC-0115 Amendment 2. The
maintainer approved it on 2026-11-16.

- **Board.** `[board.run.fvp] params`. `fvp-aemv8r-smp` states
  `bp.smsc_91c111.enabled=1` and `bp.smsc_91c111.promiscuous=1`.
- **Image.** `[image.<id>.run] nic = { tap = "<dev>" }` now also applies under
  an FVP, where it becomes `bp.hostbridge.userNetworking=0` and
  `bp.hostbridge.interfaceName=<dev>`. `"slirp"` adds nothing.
  `{ bridge = … }` is refused, because the FVP host bridge has no bridge mode.
- **Image parameters.** `[image.<id>.run.fvp] params` holds the parameters an
  image chooses, nested under the emulator as the board's are. They APPEND to
  `[image_defaults.run.fvp]`'s, as the board's are prepended to both. They are
  refused on a board whose emulator is not an FVP.
- **Carrier.** `nros build`'s west road passes all of them, the board's first,
  to Zephyr's `armfvp.cmake` as `-DARMFVP_EXTRA_FLAGS=-C;<p>;…`. It always
  passes the argument on an FVP board, empty when nothing is stated, so a
  removed parameter leaves the cache. `armfvp.cmake` appends that variable to
  the `run_armfvp` command it writes into `build.ninja`. `west build -t run`
  runs that target, and so does `nros run`, whose fvp runner adds no
  parameter of its own. No environment is read at run time.
- **Tradeoff.** The parameters are bound at configure, so changing one (a TAP
  name too) re-configures the image.

Measured on `examples/workspaces/realtime-cpp`'s `fvp` image with
`nros build fvp -- --cmake-only`, reading the `run_armfvp` command in the
generated `build.ninja`. Each line below is the tail of that command after
`-a …/zephyr.elf`:

```
board rows only:
  -C bp.smsc_91c111.enabled=1 -C bp.hostbridge.userNetworking=1
  -C bp.smsc_91c111.enabled=1 -C bp.smsc_91c111.promiscuous=1
with [image.fvp.run] nic = { tap = "tap7" },
     params = ["bp.pl011_uart1.out_file=trace.log"]:
  ... -C bp.smsc_91c111.promiscuous=1 -C bp.hostbridge.userNetworking=0
  -C bp.hostbridge.interfaceName=tap7 -C bp.pl011_uart1.out_file=trace.log
image rows removed again: back to the board rows only
```

The first pair is Zephyr's own (`CONFIG_ETH_SMSC91X`). The image's TAP
parameters come after Zephyr's `userNetworking=1`, so they win.

Tests:
- `nros-orchestration-ir`: `fvp_flags_are_the_board_then_the_nic_then_the_image`,
  `fvp_flags_refuse_a_bridge_a_bad_param_and_params_off_an_fvp`,
  `a_board_run_fvp_table_needs_an_fvp_and_well_formed_params`.
- CLI: `an_fvp_image_hands_its_model_parameters_to_the_configure`.
  Mutation-checked: with the configure argument removed, it fails.
- `nros_runner_fvp`: `the_runner_adds_no_model_flag_of_its_own`.

The model was not booted. This host has no FVP installed (`ARMFVP-NOTFOUND`
in the measured line), so the network half is the consumer's check on its
own model.
