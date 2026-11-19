---
id: 2664
title: "A native_sim image is linked with no unwind search table, so it aborts at a thread exit, silently and by layout"
status: resolved
type: bug
severity: high
area: [zephyr, build, testing]
found: 2026-11-17
related: [2724, phase-559]
---

**Home:** [phase-559](../../roadmap/phase-559-ci-workflow-organization.md)
(it blocked W5b's hosted tier-1 zephyr shard). Owner: nros 175.

## What happens

Run 43951178924 (W5b's branch nightly) failed two Zephyr cells on the hosted
tier-1 zephyr shard, `example_e2e::case_20_xrce_c_pubsub_e2e` and
`case_23_xrce_c_service_e2e`, as "the target exited with code 1 after 0.4 s"
with nothing on the console. Built locally at 1037efabd0, both fail solo, every
time, while `case_26_xrce_c_action_e2e` passes.

Run directly, the listener image dies of SIGABRT during boot. The abort is in
libgcc's `_Unwind_Find_FDE`, under native_sim's `nct_exit_and_cleanup` →
`pthread_exit`, which unwinds the exiting thread's stack.

The image's `.eh_frame_hdr` is 8 bytes: the header with no search table. ld
said so at build time, as a warning, and linked anyway:

    /usr/bin/ld: error in …/zephyr/NSI/…/zephyr.elf.loc_cpusw.o(.eh_frame); no .eh_frame_hdr table will be created

With no table, the unwinder walks `.eh_frame` in order, and aborts if it meets
the entry ld could not parse before the FDE it is looking for. That depends
on where code lands, so which images die moves with unrelated changes. That
is why the action images survive, and why the same cells passed on
2026-11-15 (hosted run 43773321880) and 2026-11-16 (self-hosted run
43816955034).

## Cause

A native-simulator board (`CONFIG_NATIVE_LIBRARY`: native_sim,
native_sim/native/64, nrf_bsim) links `zephyr.elf` as a relocatable (`-r`),
and `scripts/native_simulator/Makefile` links the executable from it. Zephyr's
`toolchain_ld_base` adds `--gc-sections` to every link, the `-r` one included.
Garbage collection in a relocatable link discards sections and leaves their
FDEs behind with `R_X86_64_NONE` relocations: the listener's `zephyr.elf` has
3260 of them in `.rela.eh_frame`. Host ld 2.38 then cannot parse that
`.eh_frame` when it links the executable.

Measured on the listener's own link command:

| `zephyr.elf` pre-link | table in the executable |
| --- | --- |
| as built (`-r --gc-sections`) | none (8-byte header) |
| the same command without `--gc-sections` | 0x2784 bytes, boots |
| as built, minus any one input archive | none, except without the Rust staticlib (`libnros_cpp.a`) |

Every object of every input is parsable on its own; a member bisect of the
staticlib leaves a combination of the Rust `nros_c` codegen units, not one
object. So the input is legitimate and the relocatable GC is what breaks it.

**The defect is old.** Run 43773321880's fixture logs carry the same ld line
on the same eight leaves: `build-ws-c-{talker,listener,service-server,
service-client,action-server,action-client}-xrce`, `build-c-talker-xrce` and
`build-logging-smoke`. No zenoh leaf has it. Those images had no unwind table
when they passed too.

Not measured: whether a newer binutils parses the same `zephyr.elf`. No newer
x86_64 ld exists on this host or in the CI images (both Ubuntu 22.04, ld 2.38),
and the fix below does not depend on the answer.

## Fix

- **Our module, not vendored Zephyr:** `zephyr/CMakeLists.txt` adds
  `-Wl,--no-gc-sections` to the link under `CONFIG_NATIVE_LIBRARY`. It comes
  later in the link line than the base option, so it wins. The executable's
  own link does no garbage collection, so the image keeps what the
  relocatable kept; measured +5 % on the listener.
- **The gate:** `scripts/build/check-unwind-table.py`. The fixture build
  (`zephyr-fixture-run-one.sh`) refuses a `zephyr.exe` that has `.eh_frame`
  and no search table, quoting ld's line from the leaf's log. The refusal
  comes before the freshness signature, so a refused leaf never reads as
  fresh. `just check unwind-table` runs the check's controls on the fast line.
- Considered and not taken: building the Rust staticlib without unwind tables
  for native_sim. Its profile is `panic = "abort"` (`nros-relwithdebinfo`), so
  no Rust panic unwinds, but a thread exit's forced unwind walks whatever
  frames the exiting thread holds, Rust ones included, and how libgcc treats
  a frame with no FDE there would need proving at boot and at every thread
  exit. The link fix keeps every FDE and removes what ld could not parse.
- Also not taken: rewriting `zephyr.elf`'s `.eh_frame` after the pre-link to
  drop the discarded sections' FDEs, which would keep the GC. A binary
  rewriter is code to maintain against every binutils change (coordinator
  ruling, 2026-11-17).

## What native_sim stops seeing

native_sim no longer garbage-collects anything, so it is no longer a canary
for an unreferenced symbol the LINK drops. The backend-registration family
(phase-531 W6: an image boots with an empty RMW registry) has two halves, and
they move apart:

1. **rustc's staticlib dead-code elimination** leaves the export out of the
   archive itself. ld flags do not touch that, so native_sim still shows it:
   `zephyr.rs`'s `assert_registers_cyclonedds` reads the image, which now
   holds what the archives hold, and `staticlib_duplicate_symbols` reads the
   C umbrella archive's `-u`-forced link (`nros_rmw_zenoh_register`).
2. **`--gc-sections` dropping an unreferenced register entry** is a link
   fact. It moves to the mps2 Zephyr image, one garbage-collected link like a
   real board's: `zephyr_cortex_m_qemu.rs`'s pubsub cells assert
   with `nm`, before they boot, `nros_app_register_backends` and
   `nros_rmw_zenoh_register` for C and C++, and for Rust (whose direct
   registration call the linker inlines, so neither name survives)
   `nros_rmw_cffi_register_named`, which only a registration calls. These
   cells are `ZephyrQemuCortexM`'s; their
   daily job is issue 2462's `zephyr-board` (in review alongside this).

Size: native_sim images grow by what the pre-link used to discard (+5 % on
the XRCE C listener). No committed baseline records a native_sim size; the
one native_sim number in the tree, RFC-0108's `.bss` delta, is a historical
measurement and stays as written.

## Also dropped

A PR to put the image's console log into the cell's panic was planned from
the first reading ("exits 1 with no output"). It would have added nothing:
the console IS the `nros run` verb's stdout, which the panic already prints,
and the image printed nothing because it aborted inside the unwinder.

## Sweep

`CONFIG_NATIVE_LIBRARY` is selected by native_sim (both `native` and
`native/64`) and nrf_bsim, on both Zephyr lines (3.7 and 4.4 partial-link
`zephyr.elf` the same way). No other board we build links a relocatable
`zephyr.elf`. The only other `-r` link in Zephyr is `CONFIG_LLEXT_TYPE_ELF_
RELOCATABLE`, which no image here enables.

## Resolved (2026-11-19)

Fixed by #4132 (de1f0447cc, on main): `zephyr/CMakeLists.txt` adds
`-Wl,--no-gc-sections` to a `CONFIG_NATIVE_LIBRARY` board's link, and
`scripts/build/check-unwind-table.py` refuses a fixture `zephyr.exe` with
`.eh_frame` and no search table (the fixture build), with its controls on the
fast line (`just check unwind-table`). Both are as the Fix section above
describes. Closed by nros 139 at the manager's request; the fix and the gate
are nros 175's.

## Second route (2026-11-19)

[Issue 2724](2724-zephyr-native-sim-link-fails-on-aarch64-host.md) found the same unparsable prelink `.eh_frame` with this fix in place: on aarch64, `--sort-section=alignment` wrote the prelink's `.rela.eh_frame` out of offset order. The nros module now turns `CONFIG_LINKER_SORT_BY_ALIGNMENT` off for native-simulator images, and `check-unwind-table.py --prelink` checks the order.
