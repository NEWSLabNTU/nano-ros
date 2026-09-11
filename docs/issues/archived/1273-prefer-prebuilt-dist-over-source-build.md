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
