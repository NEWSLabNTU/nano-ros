# Adoption probe report: industrial

Host: `ubuntu:24.04` containers (`probe-ind-1` online, `probe-ind-off` `--network none`), user `dev`.
Release installed: nano-ros **0.6.0-nros6** (newest at 16:35 UTC). Probe start 16:33:04 UTC. Full log: run-log.md.

## Verdicts
| Task | Verdict |
|---|---|
| 1 Install + build First Project on QEMU | **Done with stops.** Built and booted; `data: 'Hello World: N'` reached `ros2 topic echo` at +18.9 min. It took four off-book steps: replacing `nros run` with serial-transport.md, symlinking the zenoh-pico/mbedtls trees, installing Jazzy and switching the edition to jazzy, and adding a TCP listener to the router. |
| 2 Reproduce | **Done with stops.** Rebuilt at the same absolute path, the ELF is byte-identical (f532faa4…e178). Rebuilt in a different directory, the ELF differs (15882016 vs 15882028 B; debug info carries the path), but the `objcopy -O binary` image is identical (c367c97e…eb19). The book says nothing about reproducibility. The default image is the debug profile. |
| 3 Offline | **Done with stops**, up to the boot banner `nros QEMU Platform`/`Serial ready.`. Three network reaches: apt host prerequisites (no offline form in the book); `nros setup` → rustup-init from static.rust-lang.org, even with NROS_OFFLINE=1; `nros sync`/`nros build` → cargo git dependency github.com/NEWSLabNTU/ros-launch-manifest.git?tag=v0.2.4, even with NROS_OFFLINE=1. Workarounds: debs fetched beforehand, a 635 MB cache tarball, and CARGO_NET_OFFLINE=true set explicitly. |
| 4 Reviewer evidence | **Partly possible from the book.** Support level is stated (Verified in emulator, daily, qemu:mps2-an385), but not when it last ran. Memory evidence needs `just mem-report` (a checkout tool). I measured RAM 180,044 B, HEAP 132,816 B against the book's 64 KB, and no stack figure. There is no third-party list. The patch list is a repo link (Cyclone only), with nothing for the zenoh-pico fork. There is no attachable report. |
| 5 Long-term | **Not possible from the book.** Pins are covered (`nros pin`, forward-only). There is no support period, backport/LTS policy or asset retention; versioning is a repo link; the support model is "no commercial support entity, BDFL". |

## Stops
1. installation.md › Check the install — `nros dev sdk root` → `error: unrecognized subcommand 'dev'` (+3.0m). WA: `nros sdk-root`. Title: Book documents `nros dev …` verbs absent from release 0.6.0-nros6.
2. first-project.md › Scaffold the talker — `nros new sensor_node --board mps2-an385-baremetal --template talker` → `Error: \`nros new <name>\` requires \`--platform <p>\`` (+6.5m); with `--platform baremetal`, `--board` is refused: "`--board mps2-an385-baremetal` is for a C/C++ project". WA: `--platform baremetal` and no `--board`. The layout also differs from the book: no README.md, extra nros.toml and .cargo/config.toml, and `board = "mps2-an385"`. Title: First Project scaffold command fails on the released CLI.
3. first-project.md › Run it — `nros run --expect 'Publishing:'` → `error: unrecognized subcommand 'run'` (+7.1m). WA: `nros build` plus the manual steps in serial-transport.md. Title: Quick Start depends on `nros run`, which no release ships.
4. first-project.md › Run it (build) — `nros build` → `zenoh-pico source not provisioned at "/home/dev/.nros/sdk/nros/0.6.0-nros6/share/nano-ros/packages/rmw/zenoh/zpico-sys/zenoh-pico". Run: nros setup --source zenoh-pico …` (+7.1m). The named command answers `[already present (skip)]`. WA: symlink the store trees into the SDK root. Root cause (verified): setup provisioned under the newer fetched index's label (+c78dbffd), while the build wants the release pin 8d5ac2fe. Title: setup and build disagree on the zenoh-pico pin; the error's remedy is a no-op.
5. installation.md › Provision / vendored-tree.md › Air-gapped item 2 — `nros setup mps2-an385-baremetal --rmw zenoh` → `nros: fetched the SDK index → /home/dev/.nros/fetch/nros-sdk-index.toml` (+3.3m). It installed qemu 11.0.0-nros8, though the release pins nros7. The store dir `zenoh-pico/1.10.1+c78dbffd` contains the 8d5ac2fe tree (`.nros-provenance ref = "8d5ac2fe…"`). The book says setup "reads the index the release ships". WA (task 3): `--index <release index>`. Title: setup silently uses a newer network index and mislabels store provenance.
6. Build "next" note — `nros setup --tool qemu` → `present 11.0.0-nros8 (skip)`, then `nros locate tool qemu-system-arm` → `Error: no qemu-system-arm … (pinned 11.0.0-nros7) — provision it with \`nros setup --tool qemu\`` (+9.2m). The advice loops. WA: call the nros8 binary directly. Title: qemu pin mismatch makes the CLI's remedy loop.
7. first-project.md intro — requires "ROS 2 Humble with `ros-humble-rmw-zenoh-cpp`", which is impossible on 24.04, and the book's own ros2-interop/support-status say Humble ships no rmw_zenoh_cpp apt package. The "container the install page describes" link leads to no container (+10m). WA: Jazzy via docs.ros.org (off-book). Title: First Project prerequisite is uninstallable; dead container reference.
8. installation.md › Host prerequisites + ROS apt — `apt-get install ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp` → `trying to overwrite '/usr/lib/python3/dist-packages/catkin_pkg/__init__.py', which is also in package python3-catkin-pkg 1.0.0-2` (+10.4m). WA: `dpkg -r --force-depends python3-catkin-pkg; apt --fix-broken install`. Title: Ubuntu prerequisite python3-catkin-pkg breaks the ROS 2 apt install.
9. serial-transport.md › 2. Create the PTY pair — with that router override, `ros2 topic echo` → `Unable to connect to a Zenoh router. Have you started a router with \`ros2 run rmw_zenoh_cpp rmw_zenohd\`?` (+14m), and `nros dev sdk path qemu` → unrecognized subcommand. WA: add `"tcp/127.0.0.1:7447"` to listen/endpoints, and use `nros sdk-path`. Title: Serial quick-start router config is unreachable from the ROS 2 side.
10. first-project.md (Humble) / scaffold `ros_edition = "humble"` — the board prints `Publishing: 'Hello World: N'`, the echo connects, and **zero** `data:` lines arrive with no error (+15m; reconfirmed at +20m). WA: `ros_edition = "jazzy"`. Title: Edition mismatch delivers nothing silently; Quick Start defaults to an edition it cannot interop with.
11. Task-2 rebuild — `nros build` → `Error: 1 dependency drift(s) from the pin set (RFC-0104 D9): tool \`qemu\` is recorded as 11.0.0-nros8, the index pins 11.0.0-nros7` (+12m). The lock was written by the `nros setup --tool qemu` that stop 6 advised. WA: `nros update --tool qemu`. Title: Following the CLI's own qemu advice poisons nros-sdk.lock.
12. installation.md › Provision a whole workspace — `nros setup` → `./system.toml: \`mps2-an385\` is not an index board key and no board catalog was found` (+12m). WA: not needed after stop 11. Title: setup rejects the board key `nros new` writes.
13. vendored-tree.md › Air-gapped — no offline form for the apt prerequisites and no release tag/asset naming. `…/download/0.6.0-nros6/…` → `curl: (22) The requested URL returned error: 404` (+22m). WA: apt --download-only helper container, and `install.sh --dry-run` to learn the tag `nros-0.6.0-nros6`. Title: Air-gapped recipe omits host packages and asset naming.
14. vendored-tree.md › Air-gapped item 2 — `NROS_OFFLINE=1 nros sync` → `unable to update https://github.com/NEWSLabNTU/ros-launch-manifest.git?tag=v0.2.4 … failed to resolve address for github.com` (+26m). Also `NROS_OFFLINE=1 nros setup` with no rustup cache → `Error: provision a Rust toolchain …`, after 15 s of retries and with no remedy named. WA: explicit CARGO_NET_OFFLINE=true, plus copied ~/.rustup and ~/.cargo. Title: NROS_OFFLINE does not make cargo/rustup offline.
15. internals/measuring-static-memory.md — `just mem-report path/to/your/binary` is checkout-only (+28m). configuration.md says HEAP defaults to 64 KB, but the image's `nros_platform_mps2_an385::memory::HEAP` measures 132,816 B. No stack budget is stated. WA: arm-none-eabi-size/nm. (The local book export also lacks static-pool-inventory.md and configuration-surface.md; I read the first on the site.) Title: Memory evidence needs the source tree; documented heap default disagrees with the image.
16. support-status.md › RMW backends — the Cyclone "fork delta" is a link into the repo. There is nothing on the zenoh-pico fork (github.com/jerry73204/zenoh-pico) and no per-image third-party/SBOM list; production-readiness points at `third-party/*/LICENSE` in the source tree (+28m). Not worked around. Title: No in-book third-party inventory or patch list.
17. (no page) — no attachable report of toolchain + pins + support level. support-status.md describes main (QEMU 11.0.0-nros8), not the installed release (nros7). The board page shows a cadence but no last-run date or link (+29m). Closest substitutes: nros-sdk.lock + nros-toolchain.toml. Title: Provide a release-specific provenance/support report with last-run evidence.
18. installation.md › Pinning a version / production-readiness §8–9 — versioning is a link to `book-internals/src/dev-env/versioning.md`; "track the roadmap directory in the upstream repo"; no support window, backport/LTS policy or asset retention. "targets ROS 2 Humble today… Jazzy no public ETA" contradicts support-status, which says Jazzy is Supported (+30m). install.sh is always fetched from main. Title: No release-support/backport policy; production checklist stale on Jazzy.

## Provenance captured (task 1)
- apt (noble): cmake 3.28.3-1build7, gcc 13.2.0-7ubuntu1, git 2.43.0, python3 3.12.3, python3-empy 3.3.4-2, python3-lark 1.1.9-1, python3-catkin-pkg 1.0.0-2, python3-yaml 6.0.1, zstd 1.5.5, curl 8.5.0, pkg-config 1.8.1.
- nros 0.6.0-nros6, from GitHub releases tag nros-0.6.0-nros6; asset sha256 c2f7f676…8f9b; install.sh fetched from main (sha256 f09eeda0…6c2c at fetch time).
- rust 1.99.0 via rustup-init 1.29.1, plus thumbv7m-none-eabi.
- arm-none-eabi-gcc 13.2-nros5 (Arm GNU 13.2.rel1 repack, nano-ros-sdk release, sha256 af5b2e04…).
- qemu 11.0.0-nros8 (fork NEWSLabNTU/qemu branch nano-ros-v11.0.0-patches, sha256 340c22d5…); the release pin is nros7 (sha256 460a65ab…).
- corrosion 0.6.1-nros1 (source build; no sha recorded).
- zenoh-pico 1.10.1 @8d5ac2fe from github.com/jerry73204/zenoh-pico.
- mbedtls "3.x" @5e146ade.
- 93 crates.io crates, plus git dependency ros-launch-manifest v0.2.4.

## Missing for a five-year product
- Per-release support window, backport and security-fix policy.
- A promise that dist assets stay hosted (nano-ros-sdk releases; a personal-account zenoh-pico fork).
- A versioned install.sh.
- A release-matched support-status page and board evidence with dates.
- An SBOM and patch inventory per image.
- An edition-support horizon, and a maintainer-continuity plan.

## Filed as (2026-11-05)

| Stop | Issue |
| --- | --- |
| 1, 2, 3 | 2255 (book ahead of the latest release) |
| 4 | 2256 (store label disagrees with its contents), via 2257 |
| 5, 6 | 2257 (setup and build read different indexes); the label half is 2256 |
| 7 | 2259 |
| 8 | 2261 |
| 9 | 2262 |
| 10 | 2260 |
| 11 | 2258 |
| 12 | 2263 |
| 13 | 2264 |
| 14 | not refiled: the release lacks issue 2197's fix (archived, landed after the release); the rustup half is phase-523 "Left" |
| 15 | 2265 |
| 16 | 2266 |
| 17 | 2267 |
| 18 | 2268; its install.sh-from-`main` note is in 2255 |

Task 2's finding (byte-identical at one path; the ELF carries the path in its debug info; the book says nothing about reproducibility) was reported but not numbered as a stop, and is not filed.
