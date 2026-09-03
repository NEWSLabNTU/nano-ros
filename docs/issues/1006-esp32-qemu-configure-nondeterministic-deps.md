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
