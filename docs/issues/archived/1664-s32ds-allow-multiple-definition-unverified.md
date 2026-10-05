---
id: 1664
title: "S32DS still carries `--allow-multiple-definition`: the only remaining use, and it cannot be measured on a host with no S32DS project"
status: resolved
resolved_in: 2026-10-05
type: tech-debt
area: build, s32ds
severity: low
found: 2026-10-03
related: [1645, 1636, 1618]
---

## What

`integrations/s32ds/makefile.defs` adds `-Wl,--allow-multiple-definition` to
every S32DS link. It is the last row in `scripts/allow-multiple-def-allowlist.txt`.
Issue 1645 removed the Zephyr row, which was measured and fixed.

The S32DS use is UNVERIFIED in both directions. The generated `nros-libs.mk`
links `corrosion/*.a`, which can hold `libnros_c.a` beside `libnros_cpp.a`.
On NuttX that pairing measured 392 duplicates, 297 of them `nros_*` plus
`REGISTRY`, and it was fixed by linking `libnros_cpp.a` alone (issue 1636).
The same fix probably applies here.

## Why it was not done in 1645

S32DS 3.6.10 is installed at `~/NXP/S32DS.3.6.10`, but no S32DS project
exists on the host. A project needs a `.cproject` plus RTD, FreeRTOS and
lwIP, made by the IDE's project wizard. The CDT-generated makefiles own the
link line, so nothing short of a real project shows which archives reach the
link and what the flag masks. Changing `nros-libs.mk` without that link would
be the unmeasured edit the allowlist exists to stop.

## What closing needs

- Create an S32DS project (the README's flow) on a host that has the RTD
  packages.
- Link it with the flag removed and read the duplicate list.
- Emit one runtime archive into `nros-libs.mk` (the NuttX fix), relink
  without the flag, and drop the allowlist row.

## Resolution (2026-10-05)

Closed without a wizard-made project. The shell's own road
(`cmake -S integrations/s32ds`) only needs what its probe reads, so a
synthesized CDT-shaped project stood in: a `.cproject`, one per-TU `.args`
carrying MR-CANHUBK3's ABI (`-mcpu=cortex-m7 … -mfpu=fpv5-sp-d16`), the
in-tree FreeRTOS kernel (`ARM_CM7/r0p1`) and lwIP. Every compile and link
used S32DS 3.6.10's bundled `gcc_v11.4/gcc-11.4-arm32-eabi`. The link ran
through a `Debug_FLASH/makefile` of the stock shape `makefile.defs`
documents (`-include ../makefile.defs`; `proj.elf: $(OBJS) $(USER_OBJS)`;
`arm-none-eabi-gcc -o proj.elf @proj.args $(USER_OBJS)`), with the FreeRTOS
kernel compiled as the project's objects, every `nros_*` the runtime
archive(s) define forced with `-u`, and the app's hooks plus the lwIP socket
API stubbed.

Three things were measured, two of them not what this issue assumed:

1. **The generated `corrosion/*.a` wildcard matched NOTHING.** Corrosion puts
   the staticlibs at `<build>/nano_ros_root/packages/api/nros-{c,cpp}/`, and
   `<build>/corrosion/` holds only `required_libs/…` in subdirectories. So the
   shell as generated put no runtime on the line at all; the README told users
   to append `nros-c` / `nros-cpp` / FFI archives by hand.
2. **The flag could not have linked anything either.** `USER_OBJS` is a
   PREREQUISITE of the CDT `.elf` rule, so `USER_OBJS += -Wl,--allow-multiple-definition`
   is `No rule to make target '-Wl,--allow-multiple-definition'` (rc 2).
3. **What the flag existed to mask is real.** With both archives on the line
   (the hand-append the README prescribed) and the flag gone: **138
   duplicates, `REGISTRY` among them** (96 `nros_*`, the `rcl*`/`rmw_*`
   compat surface, the `__NROS_SIZE_*` constants). Fewer than NuttX's 392
   only because ld reports a duplicate per archive MEMBER it pulls, and the
   order here pulls fewer `libnros_cpp.a` members.

The shell also had never been built through: the `no_std` staticlibs had no
`#[panic_handler]` because nothing applied the image's ending without a
`nano_ros_entry()`. It now calls `nros_apply_panic_policy(platform …)`.

**Fix (the NuttX rule of issue 1636).** `NROS_S32DS_API` (`C` default, or
`CPP`, case-normalized) picks ONE runtime archive, emitted into
`nros-libs.mk` by `$<TARGET_FILE:nros_{c,cpp}-static>` — no glob.
`makefile.defs` carries no flag. `nros-extra-libs.mk` stays a user hook, and
a second Rust staticlib through it is now a link error. S32DS generates no
per-package message FFI staticlibs (1645's shape), so there was nothing to
consolidate there.

| link (real `make`, S32DS gcc 11.4) | forced `nros_*` | duplicates | `REGISTRY` dup | result |
| --- | ---: | ---: | --- | --- |
| old `makefile.defs` + old `nros-libs.mk` as generated | 0 | — | — | rc 2, flag is a "No rule" prerequisite |
| old, flag removed, `libnros_c.a` + `libnros_cpp.a` | 524 | **138** | yes | rc 2 |
| new, `NROS_S32DS_API=C` → `libnros_c.a` | 307 | 0 | no | links |
| new, `NROS_S32DS_API=cpp` → `libnros_cpp.a` | 524 | 0 | no | links, one `REGISTRY` in the ELF |

`scripts/allow-multiple-def-allowlist.txt` is deleted and
`check-no-allow-multiple-def` is absolute: any use in any build file fails,
and its selftest keeps the normal path (real use, `-z muldefs`, comments that
only name it). Mutation: appending the flag to
`integrations/s32ds/makefile.defs` fails the gate (rc 1, naming line 68).

**Not verified:** a wizard-made S32DS project with RTD; the real CDT
`.args`/linker script; a real lwIP; the image running on hardware.
