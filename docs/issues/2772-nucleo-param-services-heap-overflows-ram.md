---
id: 2772
title: "The stock node-interfaces default answers for every image alike: the Nucleo-F767ZI Rust action server no longer links, and an RTIC image is refused"
status: open
type: bug
severity: high
area: [orchestration, zephyr, sizing]
related: [phase-556, issue-2647, issue-2559, issue-1766, phase-590]
found: 2026-11-21
---

**Home: [phase-556](../roadmap/phase-556-ros2-compat-audit.md) W13 fallout.**
Both symptoms trace to 6229085857 ("every image serves the stock node
interfaces by default"). Neither 57c19cd7df nor 67d36cb764 moved a Rust
Nucleo image: Rust with `param_services` had a 192 KiB heap before and after
57c19cd7df, which only shrank the undeclared store from 32 slots to 8, and
67d36cb764 reads only the two ThreadX boards' rungs.

## What happens

1. **Tier 2 stops in BUILD.** `baremetal_levels_rtic_entry`
   (`examples/workspaces/rust/src/levels_bringup`, `framework = "rtic"`) is
   refused by `nros::main!`: "this system declares `param_services` but the
   RTIC framework entry does not register the runtime's parameter or lifecycle
   services". The refusal is right. The default put the stock set on an image
   whose entry cannot serve it. Every other RTIC image had opted out by hand;
   this one, added later, had not.
2. **A Nucleo-F767ZI Rust image no longer links.** The stack-floor drift row
   `action-server/rust/zenoh@nucleo-f767zi-zephyr` (a fresh `nros new` +
   `nros build`, the user's road) fails with
   `region 'RAM' overflowed by 108992 bytes` against the board's 384 KiB
   SRAM. The image now gets `param_services`, so the Kconfig heap default goes
   from 128 KiB (`default 131072 if RUST`) to 192 KiB
   (`default 196608 if RUST && NROS_CAPABILITY_PARAM_SERVICES`), and the
   executor backing grows by the 8-slot store (about 8.9 KiB a slot,
   issue 2647). Measured by the reporter: heap 196,608 B, `EXECUTOR_BACKING`
   110,224 B, main stack 77,728 B. Lowering the heap to the C images'
   112 KiB alone frees 81,920 B, short of the overflow by about 27 KiB, so no
   single default fixes it.

## Fix

1. (RTIC) The default is derived from the entry's capability:
   `nros_orchestration_ir::FRAMEWORKS_WITHOUT_NODE_SERVICES` names the
   frameworks whose entry registers no runtime node services, and
   `node_interfaces::entry_default` gives such an image `none` when no table
   states a switch and it declares no stock axis itself. A stated switch or a
   declared axis still reaches the macro's refusal.
2. (Nucleo) The Rust branch of the heap default is gone. A Rust image with
   param_services and no lifecycle gets the 112 KiB C and C++ already had:
   the branch's 114,144 B came from an image declaring both axes, and split,
   features' zephyr_rust_params on native_sim peaks at 45,840 B. The
   action-server row links with about 4.6 KiB of RAM spare, and in its Renode
   twin the heap peaks at 47,880 of 115,200 B. Still open: that row's stack
   walk rose to 109,184 B (a type-bound indirect edge, not a deeper call) now
   that the image links; the row and the drift check's handling of it are
   issue 2783's, and peak-sized stacks give the template back about 44 KB.

## Acceptance

- `baremetal_levels_rtic_entry` builds; tier 2 reaches its cells.
- The three Nucleo stack-floor drift rows build and link.
- Every in-tree small-SRAM image is checked for the same overflow.

## Status (2026-11-24)

- **Point 1, done.** Tier 2 reaches its cells again: run 45339477050 reports
  "cells ran and FAILED", past BUILD, so `baremetal_levels_rtic_entry` builds.
- **Point 2, done on the record.** The three Nucleo drift rows link:
  `config/stack-floors.toml` records each re-walked on 2026-11-22/23 after
  the 112 KiB heap. The nightly drift lane itself verified no Zephyr row from
  2026-11-21 (its SDK sits outside the SDK store and read as unpinned; fixed
  in #4782), so the next nightly with that fix is the lane's own
  confirmation.
- **Point 3, open: the small-SRAM sweep below.** Its first run is recorded
  under "Sweep result" at the end.

### The small-SRAM images

SRAM is read from each board's Zephyr devicetree (Zephyr 3.7,
`zephyr,sram`):

| Board key | Zephyr board | SRAM | In the sweep |
| --- | --- | --- | --- |
| `nucleo-f767zi-zephyr` | `nucleo_f767zi` | 384 KiB | yes |
| `rpi-pico-w-zephyr` | `rpi_pico/rp2040/w` | 264 KiB | yes |
| `s32z270-rtu0-zephyr` | `s32z2xxdc2@D/s32z270/rtu0` | 1 MiB | no |
| `mps2-an385-zephyr` | `mps2/an385` | 4 MiB | no |
| `fvp-aemv8r-smp`, `zephyr` (native_sim) | | large / host | no |

The 2772 overflow came on the user's road (`nros new` + `nros build`), so
the sweep covers every use-case template on both small boards:
`talker`, `listener`, `server` and `action-server`, each in `rust`, `c` and
`cpp`, on zenoh. That is 24 images. It also covers the one in-tree bringup
image on a small board, `examples/zephyr/c/talker`'s `[image.nucleo_serial]`.
The Pico W has no twin and no bench (`runs = "none"`), so a link check is
the only evidence it can get today.

### Recipe (a host with the Zephyr 3.7 SDK; one image at a time)

```bash
source ./activate.sh
out=build/sram-sweep; mkdir -p "$out"; : > "$out/results.tsv"
for board in nucleo-f767zi-zephyr rpi-pico-w-zephyr; do
  for uc in talker listener server action-server; do
    for lang in rust c cpp; do
      name="sram_${uc}_${lang}_${board//-/_}"
      rm -rf "$out/$name"
      ( cd "$out" && nros new "$name" --board "$board" --lang "$lang" \
          --rmw zenoh --template "$uc" ) > "$out/$name.log" 2>&1 || {
        printf '%s\t%s\t%s\tNEW-FAILED\n' "$board" "$uc" "$lang" >> "$out/results.tsv"; continue; }
      rc=0; ( cd "$out/$name" && nros build ) >> "$out/$name.log" 2>&1 || rc=$?
      over="$(grep -o "region .RAM. overflowed by [0-9]* bytes" "$out/$name.log" | head -1)"
      printf '%s\t%s\t%s\t%s\t%s\n' "$board" "$uc" "$lang" "rc=$rc" "${over:-}" >> "$out/results.tsv"
    done
  done
done
# the bringup image
rc=0; ( cd examples/zephyr/c/talker && nros build nros_zephyr_talker_c_bringup:nucleo_serial ) \
  > "$out/bringup_nucleo_serial.log" 2>&1 || rc=$?
printf 'nucleo-f767zi-zephyr\tbringup:nucleo_serial\tc\trc=%s\n' "$rc" >> "$out/results.tsv"
column -t -s $'\t' "$out/results.tsv"
```

**Passes when** every row is `rc=0` with no `RAM overflowed`. For each row
that does not, record its log tail here: the overflow in bytes, and whether
`nros new` refused the combination (a refusal naming why is an answer, not a
failure). A row that overflows is this issue's class again, and its fix is
the image's default, never a per-project knob.

### Sweep result (2026-11-25, main e5dcabd3ca, nros 241)

The recipe above, run with `--template` (an earlier copy said `--use-case`,
which `nros new` does not take).

- **`nucleo-f767zi-zephyr`: all 12 rows `rc=0`, no RAM overflow.** The
  bringup image `nucleo_serial` builds too (`rc=0`).
- **`rpi-pico-w-zephyr`: all 12 rows refused at build (`rc=101`)**, before
  any link, each with the same message from `nros-zpico-build`:
  "NROS_ZENOH_MAX_QUERYABLES=1 is below this image's DERIVED floor of 11".

That refusal is a third symptom of this issue's class: a board default that
answers for every image alike. The Pico's board package pinned
`max_queryables = 1`, written when its only image was a C talker with no
node interfaces. Every template now serves the stock node interfaces (six
parameter services per node, plus the transient-local publishers' cache
queryables), so no template could build there.

Ruling (2026-11-25): the board states what fits; the templates keep the
stock interfaces. The fix drops the board's queryable cap, so the table
derives from each image's declarations
(`CONFIG_NROS_MAX_QUERYABLES` = -1), as on the Nucleo. Whether each Pico
row then fits 264 KiB is the sweep's next answer, recorded here with each
row's RAM use.
