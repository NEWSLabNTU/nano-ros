---
id: 1273
title: "Tools that build from source do so because the index has no `dist` row
  for the host, not because they must be built — the fix is index rows, not
  CLI code"
status: resolved
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
related: [issue-1266, issue-1267, issue-1274, rfc-0014, rfc-0062, rfc-0099]
resolved: 2026-09-11
resolved_by: phase-447 C2 (RFC-0099 D4)
---

## What this was

`plan_install` already prefers a prebuilt dist over `[tool.*.source]` — a tool
source-builds only because the index carries no `dist.<host>` row for it, not
because it must be compiled. This issue surveyed the source-only tools and
decided the three open questions:

* **espflash** — pointed at esp-rs's own release assets: statically-linked
  musl on linux-x86_64 (no glibc floor at all), gnu on linux-arm64, upstream's
  macOS build on macos-arm64.
* **play_launch_parser** — DECIDED to keep building it ourselves, published
  into `NEWSLabNTU/nano-ros-sdk` on our own schedule (`build-
  play_launch_parser.sh` added there, `work/phase-447-c2-play-launch-parser-
  dist`), rather than waiting on an upstream release cycle for a repo we own.
  No release exists yet, so it stays on the dist-or-reason ratchet until one
  does.
* **cargo-binstall** — DECIDED not for now: it resolves at run time against
  unpinned crates.io/GitHub metadata, where this index's contract is a pinned
  `url` + `sha256` verified before unpack. A maintainer-side, offline use (to
  MINT a dist row, never to install one) is the shape that would keep both
  properties, if it comes back.

Extended into a full RFC-0099 D4 survey of every `[tool.*]` with no dist:
sccache also gained a musl dist (no runtime closure — `ldd` prints
"statically linked"); rustfilt, cargo-show-asm, corrosion and genromfs were
confirmed to have no upstream release asset at all; make stays `[tool.*]` by
its own prior decision; nros is circular (no release exists yet, phase-431
W5); esp32-qemu was MEASURED and declined — upstream's matching
`esp-develop-9.2.2-20260417` asset is a full GUI QEMU build (~55 shared
libraries: SDL2, X11, Wayland, PulseAudio, D-Bus, systemd) rather than the
minimal `--disable-*` closure the source recipe exists to produce.

15 of 25 tools carried a dist before this; 17 of 25 do after. The remaining 8
are tracked, each with its reason, on the dist-or-reason ratchet
(`.config/dist-or-reason-baseline.txt`, `check-dist-or-reason`) — a list that
may only shrink.

`[prereq.libpython310]`'s `why` also named a play_launch_parser dist that
never existed; corrected to name the real reason (`nros-launch-resolve`'s
`DT_NEEDED` on `libpython3.10.so.1.0`, phase-447 A1 / PR #896).

## Follow-up, 2026-09-21 — the play_launch_parser dist is seeded

The one decision above that ended "no release exists yet" now does.
`nano-ros-sdk`'s `work/phase-447-c2-play-launch-parser-dist` was
fast-forwarded onto that repo's `main` (53e2871 -> 9babb72, ancestry checked
before the push), `build-tool.yml` was dispatched with
`tool=play_launch_parser version=0.1.0-nros1 upstream=838ce948`, and tag
`play_launch_parser-0.1.0-nros1` carries one `.tar.zst` + `.sha256` per matrix
host. `[tool.play_launch_parser]` leaves the dist-or-reason ratchet with
`dist.linux-x86_64` and `dist.linux-arm64`, each `floor = { glibc = "2.34" }`
measured off the published artifact.

Two things the measurement said that the decision had not anticipated:

* The linux binaries name exactly ONE external `DT_NEEDED`,
  `libpython3.10.so.1.0` — the same hard single-minor pyo3 link
  `nros-launch-resolve` has. So `[prereq.libpython310]` acquires a SECOND
  consumer, and the sentence at the bottom of this issue ("named a
  play_launch_parser dist that never existed") is now true of its history and
  not of the present; its `why` names both binaries.
* macos-arm64 BUILT and PUBLISHED, and is deliberately not declared. Its
  `LC_LOAD_DYLIB` is an absolute
  `/Library/Frameworks/Python.framework/Versions/3.14/Python` with no
  `@rpath` — the python.org framework the `macos-14` runner happens to carry
  — so it cannot start on a mac without that exact version, and D1's backward
  half (which would refuse it) reads `ldconfig` and is Linux-only. Declared,
  the row would hand most macs a loader error INSTEAD of the source build that
  works. Same shape as `[tool.esp32-qemu]`'s decline: measured, then declined,
  with the measurement written into the index beside the entry.

The entry also gained its first `smoke` probe (measured with
LD_LIBRARY_PATH/PYTHONPATH/PYTHONHOME stripped, the way the smoke path spawns
it), so it leaves `.config/smoke-or-reason-baseline.txt` as well.
