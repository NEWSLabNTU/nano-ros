---
id: 1006
title: "esp32-qemu's configure does not disable the backends it never uses, so its
  runtime dependency set is a property of the machine that built it"
status: open
type: bug
area: tooling, build
related: [0926, 0500]
---

`[tool.esp32-qemu]` is **source-built with no dist** — every user runs the
recipe. Its `configure` disables gtk, vnc, sdl and docs, and says nothing about
audio, curl or tools. qemu's configure AUTO-DETECTS those, so the binary links
whatever backends the build host happened to have dev headers for.

`check-dist-runtime-deps` against a source build of this recipe found **69
sonames no `[prereq.*]` declares**, essentially all of them backends the
emulator never uses:

    audio   libasound, libpulse, libpulsecommon, libFLAC, libvorbis,
            libvorbisenc, libsndfile, libsndio, libasyncns, libogg
    X11     libX11, libX11-xcb, libxcb, libXau, libXdmcp, libxkbcommon
    net/misc libcurl, libssh, librtmp, libsasl2, libpsl, libsystemd,
            libudev, libusb-1.0, libdbus-1, libapparmor, …

Compare the control, same upstream project, same index:

| tool | configure disables | `system = [..]` |
| --- | --- | ---: |
| `qemu` | gtk, vnc, sdl, spice, docs, tools | 2 |
| `esp32-qemu` | gtk, vnc, sdl, docs | 3 (now 8) |

## Why this is not "declare the 69"

The set is not a property of the recipe. It is a property of the machine that
ran it: a lean container yields a short list, a developer desktop with ALSA,
PulseAudio and X11 headers yields this one. Writing those 69 into the index
would record one host's accident as though it were the recipe's contract, and
the next builder on a different host would fail the same gate with a different
list.

The five entries added alongside this issue are the opposite case and are
therefore safe: zlib, pcre2, selinux, tinfo and openssl are reached through
glib and qemu's block layer on any build, so they belong in `system = [..]`
whatever the host.

## The fix, and why it is not done here

Pin the link surface in `configure` — `--audio-drv-list=` (empty), plus the
`--disable-*` this recipe is missing relative to `[tool.qemu.source]` — so the
dependency set is decided by the recipe rather than the host. Then declare what
remains, which should be close to qemu's two.

Not done in this commit because it needs a REBUILD to verify, and a wrong flag
breaks the esp32 lane rather than failing loudly:

* `--enable-gcrypt` and `--enable-slirp` are deliberate (secure-boot crypto and
  user-mode networking) and must survive.
* `--target-list=riscv32-softmmu` is the whole point of the fork.
* Whether upstream's esp-develop branch accepts every flag `[tool.qemu.source]`
  passes is unverified — the fork is pinned at an older base, which is also why
  it carries `--disable-werror` (see the comment above `configure`).

## How to verify a fix

Rebuild via `nros setup --tool esp32-qemu` and re-run
`python3 scripts/check-dist-runtime-deps.py`. The gate needs a provisioned
store, is in no registry, and SKIPS loudly without one — so it is only
observable on a host that has actually built this tool. That is why the gap sat
unnoticed: CI never provisions esp32-qemu (it is nightly-only), and a developer
who has built it sees a 75-line failure that looks like index rot rather than a
configure problem.

## What landed (the configure is pinned; the closure is NOT re-measured)

`[tool.esp32-qemu.source].configure` now names 69 flags instead of 13. The added
ones are all `--disable-*`, grouped as UI, audio, host-device passthrough,
block drivers beyond the raw image, TLS, and the remaining host libraries — the
whole set of qemu feature options that reach for a system library and that
`-M esp32c3 -icount 3 -nographic -drive file=…,if=mtd,format=raw -nic
user,model=open_eth` never touches. `--enable-gcrypt`, `--enable-slirp`,
`--target-list=riscv32-softmmu` and `--disable-werror` are untouched; `fdt`,
`pixman` and `zlib` are left auto deliberately (the riscv boards require fdt,
and zlib is `required: true`).

**Measured, without a build:** every one of those 69 flag tokens is accepted by
the pinned tag. Checked against the fork's own generated
`scripts/meson-buildoptions.sh` and `configure`'s case list at
`esp-develop-9.2.2-20260417` — an unrecognised option is `ERROR: unknown
option`, so this is the failure mode the "not done here" note was worried about,
and it is now excluded statically rather than hoped away.

**NOT measured:** what the resulting ldd closure is. That still needs
`nros setup --tool esp32-qemu` + `check-dist-runtime-deps`, exactly as "How to
verify a fix" says. The issue therefore stays open until someone rebuilds.

## Two corrections to the reasoning above

**`--audio-drv-list=` alone would not have worked.** `audio/meson.build` at the
pinned tag builds an audio module for every driver whose dependency `.found()`
— `foreach m : [['alsa', alsa, …], ['pa', pulse, …], …] if m[1].found()` — and
not for the drivers named in `audio_drv_list`. An empty list changes
`CONFIG_AUDIO_DRIVERS`, i.e. what the RUNTIME selects, and leaves libasound and
libpulse (with libpulse's libsndfile/FLAC/vorbis/ogg/X11 tail) linked. The
per-driver `--disable-alsa --disable-pa --disable-jack --disable-oss
--disable-sndio --disable-pipewire` is what cuts the link surface, and once they
are off the default list resolves to empty anyway — so the flag the fix
prescribed is redundant, not merely insufficient, and is not in the new line.

**The control table compares two different things.** `[tool.qemu]`'s
`system = [..]` is 2 because its DIST bundles its closure into `lib/` with
`RUNPATH=$ORIGIN/../lib` (18 libs; the `-nros6` comment in the index measures
external closure 20 → 2), not because its configure is tighter.
`[tool.qemu.source]`'s configure says nothing about audio, curl or tools' block
drivers either — it is the same auto-detection, and `ci/nano-ros-sdk/scripts/
build-qemu.sh` mirrors it flag for flag. The real difference is that qemu ships
a dist for the three host keys and esp32-qemu ships none, so nobody but the
release job runs qemu's configure while EVERY esp32 user runs this one. That is
the property the new gate keys on.

## Gate coverage (issue-0196 rule)

`check-dist-runtime-deps` does cover this tool — a source build installs into
the same `<store>/<tool>/<version>` it walks, which is how the 69 were found —
but only AFTER a provision, and CI never provisions esp32-qemu. So the gate
could not have caught the defect, and cannot catch a future flag deletion
either. `scripts/sdk/check-qemu-source-features.py` (wired into
`just check sdk-index` and the `sdk-index` job in `gate.yml`) is the static
half: for a `[tool.*]` whose `source.git` names qemu, whose `source.configure`
runs `./configure`, and which publishes NO `dist.*`, every host-library feature
must be pinned by name. `[tool.qemu]` is exempt because it has a dist. Negative
control run: against this file's pre-fix revision the gate reports 55 of its 58
host-library features unpinned and exits 1; against the new one it passes.
