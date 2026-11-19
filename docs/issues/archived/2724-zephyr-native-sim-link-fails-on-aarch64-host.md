---
id: 2724
title: "On an aarch64 host the Zephyr `native_sim/native/64` image of `zephyr-byo/app` fails to link: three undefined network symbols"
status: resolved
type: bug
severity: low
area: [zephyr, build, templates]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "fix(zephyr, issue 2724): a native-simulator image does not sort its prelink by alignment"
related: [2590, 2664]
---

## Symptom

`just check template-copy-out` on an aarch64 Linux host (`uname -m` =
`aarch64`) fails on one template, `zephyr-byo/app`, the only copy built for
`native_sim/native/64` (with the `nros-nsos` snippet). Every other template
passes on the same host, including the four Zephyr `mps2/an385` copies.
`west build` compiles and fails at the native-simulator link:

```
zephyr/lib/posix/options/net.c:222: undefined reference to `zsock_getnameinfo'
zephyr/subsys/net/l2/ethernet/ethernet.c:934: undefined reference to `net_mgmt_NET_REQUEST_ETHERNET_SET_TXINJECTION_MODE'
zenoh-pico/src/link/transport/udp/address.c:38: undefined reference to `_z_udp_unicast_endpoint_init'
collect2: error: ld returned 1 exit status
```

(The `.eh_frame` "no .eh_frame_hdr table will be created" lines above
them are noise.)

## Not caused by a change: main reproduces it

On 2026-11-19 it first showed on PR #4288 (issue 2590). The same template,
copied from main at the PR's merge base and built with that tree's own `nros`
(`nros configure && nros build` in the copy), fails with the same three
symbols. 2590 touches nothing in Zephyr networking or zenoh-pico.

## Where to look

- The three symbols come from three layers: Zephyr's POSIX `getnameinfo`
  shim, the ethernet L2, and zenoh-pico's UDP unicast transport. That points
  at a Kconfig set that differs on this host (sockets, offload, L2 or
  zenoh-pico feature flags), not at one missing object. Read the build's
  merged `.config` beside an x86_64 build of the same copy.
- `native_sim/native/64` on an arm64 Linux host is less exercised than on
  x86_64. Confirm the board and the `nros-nsos` snippet are supported there
  before treating this as our defect. If they are not, the template check
  should refuse `native_sim` on this host as an unmet precondition rather
  than report a broken template.

## Effect

`template-copy-out` is red on every aarch64 host whatever the change, so a
local run there cannot vouch for `zephyr-byo/app`. CI runs on x86_64 and is
unaffected.

## Resolution (2026-11-19)

Neither a Kconfig default that differs by arch nor a dropped zenoh-pico
source. Measured on this host, the chain is:

1. The native simulator links `zephyr.elf` as a relocatable (`-r`), with
   Zephyr's `--sort-section=alignment` (from `CONFIG_LINKER_SORT_BY_ALIGNMENT=y`).
2. That prelink's `.eh_frame` CONTENT is byte-identical to an unsorted
   link's (898 CIEs, 9041 FDEs, the same listing). Its `.rela.eh_frame`,
   though, is written OUT OF OFFSET ORDER: one descent, 0x55644 then 0x44,
   of 9129 relocations. The unsorted link has none.
3. BFD's `.eh_frame` parser requires those relocations in order, so the
   executable's link reports "error in zephyr.elf(.eh_frame); no
   .eh_frame_hdr table will be created".
4. With the section unparsed, the executable's `--gc-sections` keeps every
   function `.eh_frame` names. Three dead functions (`getnameinfo()`, the
   ethernet TX-injection helper and
   `_z_udp_unicast_endpoint_init_from_address`) reference code that is not
   in the image, which gives the three undefined symbols.

How it was found:
- A leave-one-out over the prelink's inputs and options: the failure needs
  the Rust staticlibs plus any whole-archived Zephyr C library, and goes away
  when ONLY `--sort-section=alignment` is removed.
- A minimal object with sdata8 personality and LSDA encodings links clean, so
  the Rust objects' 8-byte pointers are not the trigger, as first suspected.
- Why only aarch64 shows it is not established. Every `.eh_frame` input is
  8-aligned with a size divisible by 8, so it is not padding.

It is the class of issue 2664 (an unparsable `.eh_frame` out of the same
prelink, fixed by `-Wl,--no-gc-sections`) by a second route; that fix is in
this build.

Fix:
- `zephyr/Kconfig` adds a default to Zephyr's symbol:
  `config LINKER_SORT_BY_ALIGNMENT` / `default n if NROS && NATIVE_LIBRARY`.
  Modules are sourced before `Kconfig.zephyr`'s own `default y`, and the first
  default that holds wins. So it reaches every nros native-simulator image,
  the bring-your-own template included, with no per-leaf conf, and a conf
  that sets the symbol still overrides it. Measured on the `zephyr-byo` copy:
  none of the merged fragments (`prj.conf`, `nsos.conf`, `zephyr.conf`,
  `nros-module-kconfig.conf`) sets it, `.config` reads `# CONFIG_LINKER_SORT_BY_ALIGNMENT
  is not set`, the image links, and it boots to its zenoh connect.
- `scripts/build/check-unwind-table.py --prelink` refuses a relocatable whose
  `.rela.eh_frame` (or `.rel.eh_frame`) is out of offset order. On the two
  real prelinks it refuses the sorted one (naming 0x44 after 0x55644) and
  passes the unsorted one. Its self-test gains an ordered prelink (the
  negative control) and the same prelink with two relocations swapped
  (7 cases). `zephyr-fixture-run-one.sh` runs it on every native-simulator
  build's `zephyr.elf`, whether or not the final link succeeded.

x86_64: the default applies there too. The executable's size and a boot
smoke, main against the fix, are measured on an x86_64 host before merge (see
the pull request); if x86_64 changes, the default is scoped then.

