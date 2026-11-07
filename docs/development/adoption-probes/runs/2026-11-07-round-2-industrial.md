# Adoption probe, round 2: industrial

- **Host:** Docker containers on a Linux 6.8 x86_64 machine. The online container was `ubuntu:24.04` (`probe2-ind`). The offline container was `ubuntu:24.04` started with `--network none` (`probe2-ind-off`). All work ran as the non-root user `dev`, which had passwordless sudo.
- **ROS 2:** Jazzy (`ros-jazzy-ros-base 0.11.0-1noble.20261006`, `ros-jazzy-rmw-zenoh-cpp`), installed from the ROS apt repository as the ROS documentation that Install links to describes.
- **Release installed:** `0.6.0-nros7` (`nros-install: installed 0.6.0-nros7`). `nros version` prints `nros 0.6.0`, `codegen version 21`, `play_launch 8d2c78d4…` and `ros-launch-manifest v0.3.12`.
- **Source:** only the book sources (the r2 copy of `book/src`). I did not read the repository.
- **Start time:** 2026-11-07T08:55:51+08:00. Every time below is wall clock since this start (T+mm:ss).

## Verdicts

| Task | Verdict | One line |
| --- | --- | --- |
| 1. Install and build the first example on QEMU | **done with stops** | The scaffolded talker does not compile on 0.6.0-nros7 (stop 2). After a per-image knob it boots and prints `Publishing:`. `ros2 topic echo` on Jazzy receives nothing (stop 3). |
| 2. Reproduce the image | **done with stops** | A clean rebuild at the same path is byte-identical (sha256 `55f96fa6…`). A rebuild of identical sources in another directory differs, in the loadable bytes too. The book says nothing about why (stop 5). |
| 3. Install offline | **done with stops** | Reached `Publishing: 'Hello World: 1'` with no network. The book's .deb recipe does not cover a pristine Ubuntu (stop 6). A run without `NROS_OFFLINE=1` fails on crates.io with no hint (stop 7). |
| 4. Reviewer evidence | **done with stops** | The book gives the support level, the cadence, the third-party table, the fork patch lists and `nros doctor --report`. It gives no last-run date (stop 8). The memory report needs a checkout (stop 9), and the report is incomplete (stop 12). |
| 5. Long-term question | **done with stops** | The book answers clearly: only the newest release is supported, nothing is backported, and assets stay published. That is not enough for a five-year product (stop 13). |

## Stops

1. **Tutorial › First Project › Scaffold / Run.** T+06:15.
   - Command: `nros new sensor_ws --board mps2-an385-baremetal --template talker` (the chapter sources ROS 2 only later, at Run).
   - Decisive output: `ROS 2 edition: humble (the default; source a ROS 2 setup.bash or pass --ros-edition to pick another)`. Later, `nros run` printed: `warning: this image speaks ROS 2 humble, and the shell's ROS 2 is jazzy … Set ros_edition = "jazzy" in the bringup's system.toml`.
   - Why it is a stop: the chapter says "on Jazzy, write `jazzy` where they say `humble`". By the time the reader sources Jazzy, the scaffold has already written `ros_edition = "humble"`. The chapter skips this step.
   - Workaround: I edited `system.toml`. Sourcing Jazzy before `nros new` also works (it picks `jazzy (the sourced ROS 2, $ROS_DISTRO)`).
   - Suggested issue: "First Project scaffolds before sourcing ROS 2, so a Jazzy host gets a Humble image".
2. **Tutorial › First Project › Run.** T+07:36.
   - Command: `source /opt/ros/jazzy/setup.bash; nros run sensor_node --expect 'Publishing:'`.
   - Decisive output: `error[E0080]: evaluation panicked: nros::main!: the SystemModel registers 1 callback entities but the executor callback table compiled in is smaller (board mps2-an385-baremetal ignores per-entry sizing). Raise NROS_EXECUTOR_MAX_CBS (e.g. to 3) in the entry's .cargo/config.toml [env] and rebuild from clean … issue 0257`.
   - I reproduced it on a second fresh scaffold with Jazzy sourced (T+23:52), so it does not depend on the edition.
   - The build log shows `resolved … (max_cbs 0, nodes 1)` and `metadata/sensor_node.json: stale against its sources (re-run nros configure)`. Running `nros configure` printed `up to date` and changed nothing.
   - Why it is a stop: the book's first example fails to compile as written. The message points at a generated file and at an issue number, neither of which is in the book.
   - Workaround: from Configure an image › Per-image build knobs, I added `[image.baremetal.knobs.executor] max_cbs = 4` to the bringup `system.toml`. The talker then booted and matched after 3.2 s.
   - Suggested issue: "mps2-an385-baremetal talker scaffold fails to compile: executor callback table derived as 0 (max_cbs 0)".
3. **Tutorial › First Project › See it in ROS 2.** T+21:44 to T+25:44, with several attempts from T+08:12.
   - Command: `export RMW_IMPLEMENTATION=rmw_zenoh_cpp; ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort`, run while `nros run sensor_node` was streaming.
   - Decisive output: nothing within 25 to 30 s, with `PYTHONUNBUFFERED=1`. `ros2 topic info /chatter -v --no-daemon` shows `Publisher count: 1`, `Topic type hash: INVALID` and `Reliability: RELIABLE`.
   - The result was the same for the hand-edited Humble→Jazzy workspace and for a workspace scaffolded with Jazzy sourced.
   - The chapter also says "the talker publishes best-effort", but the graph reports RELIABLE.
   - Why it is a stop: the chapter's interop check fails on the Ubuntu 24.04 + Jazzy combination that Install names as supported.
   - Workaround: none found from the book.
   - Suggested issue: "mps2-an385-baremetal zenoh talker publishes with type hash INVALID; Jazzy ros2 topic echo receives nothing".
4. **Install › Provision your board, and Task 1 provenance.** T+06:07.
   - Command: `nros setup mps2-an385-baremetal --rmw zenoh`, then reading the lock files.
   - Decisive output: `~/nros-sdk.lock` (written in the directory setup ran from) records sha256 for arm-none-eabi-gcc and qemu, but records corrosion as `provenance = "source"` with no hash. It has no entry for Rust, rustup, zenoh-pico or mbedtls.
   - The workspace `nros-sdk.lock` has an empty `[tool]` table. It records only the zenoh-pico commit, a cargo registry digest and the ROS package versions.
   - mbedtls is reported only as `3.x` (commit `5e146ade…`).
   - rustup 1.29.1 and Rust 1.99.0 were installed with no download URL printed.
   - The build log also dumps raw `cargo:rerun-if-env-changed=…` lines into setup output.
   - Why it is a stop: the book cannot tell me where every tool came from. Install › Working offline says "the index names every asset's upstream", but the index lives inside the SDK root and no page shows it.
   - Workaround: I assembled the list from the setup log, `nros setup --dry-run` and `nros doctor --report` (see the appendix).
   - Suggested issue: "nros-sdk.lock omits Rust, source packages and corrosion hash; workspace lock [tool] is empty".
5. **Task 2, reproducibility.** The book has no section on it. Choose your path promises "the same inputs give the same image". T+26:27.
   - Command: build the same scaffold, with the same knob edit, in `~/b/sensor_ws`, then compare it with `~/a/sensor_ws`.
   - Decisive output: `55f96fa6…` (a) against `6fc6b81d…` (b). The `objcopy -O binary` loadable images also differ (`cmp: differ: char 61`). `strings` finds 69 occurrences of `/home/dev/a` in image a.
   - Moving a's `build/` away and rebuilding at the same path gave `55f96fa6…`, byte-identical.
   - Why it is a stop: the image depends on the absolute workspace path, and the book never says so. It also gives no path-remapping option.
   - Also: the default build is the cargo `dev` profile (`Finished dev profile [unoptimized + debuginfo]`), a 16 MB ELF. First Project does not say this. Build profiles is the only place it is addressed.
   - Workaround: build at a fixed absolute path.
   - Suggested issue: "Images embed the absolute workspace path; rebuild in another directory is not byte-identical, and the book does not say so".
6. **Install options › Working offline › 1. Carry the host packages.** T+28:31, then the install failed at T+34:13.
   - Command: the book's `docker run --rm -v "$PWD/debs:/debs" ros:jazzy-ros-base bash -c 'apt-get install -y --download-only … ros-jazzy-rmw-zenoh-cpp …'`.
   - Decisive output: it downloads only 10 .debs (2 of them ROS). `ros-jazzy-ros-base` is missing, and so is everything preinstalled in the `ros:jazzy-ros-base` image, such as ca-certificates. The step then says "after installing ROS 2 the same way", which has no offline meaning on a pristine Ubuntu.
   - Why it is a stop: the recipe does not carry ROS 2 itself.
   - Workaround: I downloaded the full set (534 .debs, 206 MB, including `sudo` and `ros-jazzy-ros-base`) in an `ubuntu:24.04` container with the ROS apt source. The first offline `apt-get install ./*.deb` then failed with `libssl-dev : Depends: libssl3t64 (= 3.0.13-0ubuntu3.16) but 3.0.13-0ubuntu3.15 is to be installed` and `python3-rosdistro-modules : Depends: ca-certificates but it is not installable`. I fetched those three debs with `apt-get download` and retried, and the install succeeded at T+39:05. Part of this was my own download container's fault, but it shows the book's recipe does not address a base image that differs from the one it downloads in.
   - Suggested issue: "Working offline .deb recipe uses ros:jazzy-ros-base and omits ROS 2 itself and base-image packages for a pristine Ubuntu".
7. **Install options › Working offline › 6. Build with `NROS_OFFLINE=1`.** T+42:11.
   - Command: `nros run sensor_node --expect 'Publishing:'` in the offline container, without `NROS_OFFLINE=1`.
   - Decisive output: `Error: refresh source metadata for sensor_node … metadata-mode harness failed (exit 101) … failed to get log as a dependency … Could not resolve host: index.crates.io`.
   - Why it is a stop: the error does not name `NROS_OFFLINE=1`, even though `nros` can see that the host has no network. This is the only step that tried to reach the network. `nros setup` was fully offline-clean (`all packages already present`).
   - Workaround: `NROS_OFFLINE=1` from the book. The run then matched after 3.4 s (T+44:25).
   - Suggested issue: "nros run fails on crates.io resolution offline without naming NROS_OFFLINE=1".
8. **Reference › Board pages › nros-board-mps2-an385, and `nros board show`.** T+38:52.
   - Command: `nros board show mps2-an385-baremetal`, `nros doctor --report`, and the board page.
   - Decisive output: `status Verified in emulator (daily) … cadence daily`. No page and no command gives the date of the last run, a link to the run, or its result.
   - "Daily" verification also does not fit stop 2: the book's own first-project scaffold for this board does not compile on the current release.
   - Why it is a stop: "when it was last run" cannot be answered from the book.
   - Workaround: none.
   - Suggested issue: "Board support pages and nros board show give cadence but not the date/result of the last verifying run".
9. **Contracts and sizing › Measure static memory.** T+38:08.
   - Command: the book's `llvm-size -A` and `llvm-nm --size-sort` recipe, which works after `rustup component add llvm-tools`. That command needs the network once and is not covered by the offline list.
   - Decisive output: `.data 1984`, `.bss 173188`, so RAM is 175,172 B. `nros_platform_mps2_an385::memory::HEAP` is 132,816 B, with type `b`, which means `.bss`.
   - The book says the heap "lands in `.data`, not `.bss`", but the measurement shows it in `.bss`. The attributed memory report "ships in the nano-ros source tree, not in a release" and the book sends the reader to Internals for it.
   - Stack: per the book there is "no budget" and no guard.
   - Why it is a stop: the reviewer-grade attribution needs the source tree, and the book's heap-section claim is contradicted by the measurement.
   - Workaround: the llvm-tools totals only.
   - Suggested issue: "Ship the memory report in the release; correct the HEAP section claim for mps2-an385-baremetal".
10. **Reference › Third-party code › Rust crates.** T+38:30.
    - Command: `cargo metadata … | jq …`.
    - Decisive output: `jq: not installed`. jq is not in Install's host packages.
    - Workaround: a python3 one-liner. It listed 140 packages, build-time ones included.
    - Why it is a stop: the command fails as written on the book's own install.
    - Suggested issue: "Third-party crate listing needs jq, which Install does not install".
11. **Reference › Third-party code › C and C++ sources.** T+38:40.
    - The table says Mbed TLS is compiled in only "with TLS (the link-tls feature)", yet `nros setup` provisions mbedtls for this board, and nothing in the book tells me whether this image has `link-tls`. So the image's C third-party list stays a guess.
    - The zenoh-pico patch list is in the book: fork `jerry73204/zenoh-pico`, branch `nano-ros-1.10.1`, 81 commits and +9641/−214 lines on top of 1.10.1. Its "Reproduce" line needs `gh api` against GitHub. The fork lives on a personal GitHub account, which a security reviewer will flag.
    - Workaround: none from the book.
    - Suggested issue: "Third-party code should state per image which C components were compiled (e.g. via nros doctor --report)".
12. **Reference › Support status › This page and your release (`nros doctor --report`).** T+38:52.
    - Command: `nros doctor --report` in the bringup package.
    - Decisive output: a dated report with `nros 0.6.0`, board status and cadence, arm-none-eabi-gcc 13.2-nros5, qemu 11.0.0-nros8, `zenoh-pico 1.10.1: source https://github.com/jerry73204/zenoh-pico.git@c78dbffd…`, `mbedtls 3.x: source …@5e146ade…` and corrosion 0.6.1-nros1.
    - Missing from it: the full release tag (it prints 0.6.0, not 0.6.0-nros7), the Rust toolchain (1.99.0), the ROS 2 edition and `rmw_zenohd` version, tool sha256s, the crate list, and any hash of the built image.
    - Why it is a stop: it is the one document the book offers to attach to a review, and it cannot be attached alone.
    - Suggested issue: "nros doctor --report: add release tag, Rust toolchain, ROS/rmw versions, hashes and image digest".
13. **Reference › Support status › Release support, and Install options › Pinning a version.** T+44:30.
    - The book says: "Only the newest release is supported … nothing is backported … Release assets stay published". It also says nano-ros is pre-1.0.
    - Missing for a five-year product:
      - an LTS line, or any support window;
      - a security or vulnerability reporting and advisory process ("security", "CVE" and "SBOM" appear nowhere in the book);
      - an end-of-life policy;
      - how long the pinned Rust 1.99.0 channel and the GitHub-hosted forks stay fetchable;
      - escrow or mirroring guidance for the forks beyond "rehost those URLs";
      - how a pin relates to the ROS 2 distro's own EOL.
    - Workaround: none. This is a policy gap.
    - Suggested issue: "Book has no long-term support, security advisory or EOL policy for products that ship for years".

## Appendix: run log

Times are host local (+08:00).

### Task 1

- 08:56:08 to 08:56:24. Container `probe2-ind` started; user `dev` created with passwordless sudo.
- 08:56:32. Installed the ROS apt source per docs.ros.org. The `api.github.com` lookup for the latest `ros-apt-source` was rate-limited and returned an empty `.deb` (`dpkg-deb: error: unexpected end of file`). This is a problem with the ROS docs, not with nano-ros. I read the version (1.3.0) from the releases redirect instead.
- 08:57:27 to 08:58:45. `apt-get install ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp` succeeded.
- 08:58:52. Install step 1: the host packages installed fine.
- 08:59:16. Install step 2: `curl … install.sh | sh` printed `nros-install: installed 0.6.0-nros7` from `https://github.com/NEWSLabNTU/nano-ros/releases/latest/download/nros-linux-x86_64.tar.zst`.
- 08:59. `nros version` printed `nros 0.6.0`, `codegen version 21`, `play_launch 8d2c78d4…` and `ros-launch-manifest v0.3.12`.
- 08:59:26 to 09:01:58 (2.5 min). `nros setup mps2-an385-baremetal --rmw zenoh` printed `ready; locked in /home/dev/nros-sdk.lock`. It installed:
  - rustup 1.29.1 with Rust 1.99.0 (`1.99.0-x86_64-unknown-linux-gnu`, "latest update on 2026-10-01"), plus the target `thumbv7m-none-eabi`;
  - arm-none-eabi-gcc 13.2-nros5, a prebuilt dist (201.6 MB in 1 m 35 s), sha256 `af5b2e04…`;
  - qemu 11.0.0-nros8, a prebuilt dist (66.4 MB), sha256 `340c22d5…`;
  - zenoh-pico 1.10.1+c78dbffd, as source from the fork `https://github.com/jerry73204/zenoh-pico.git`;
  - mbedtls "3.x"+5e146ade, from `https://github.com/Mbed-TLS/mbedtls.git`;
  - corrosion 0.6.1-nros1, built from source with "no prebuilt for linux-x86_64", using host cmake and GCC 13.3.0. The message said `1 installed package(s) declare no smoke probe … corrosion`.
- From apt, per the book: git, cmake, gcc 13.3 (build-essential), python3 and the rosidl Python packages. `rmw_zenohd` came from `ros-jazzy-rmw-zenoh-cpp` (lock: `rmw_zenoh_cpp 0.2.11`, `std_msgs 5.3.8`, `builtin_interfaces 2.0.4`).
- 09:02:06. `nros new sensor_ws --board mps2-an385-baremetal --template talker` printed `ROS 2 edition: humble (the default …)`. This is stop 1.
- 09:02:11 to 09:03:27. `nros run sensor_node --expect 'Publishing:'` (1 m 16 s) failed with `error[E0080] … NROS_EXECUTOR_MAX_CBS`, rc 2. This is stop 2.
- 09:03:39. `nros configure` printed `configure: up to date`; the rerun failed the same way.
- 09:03:49 to 09:03:55. I set `ros_edition = "jazzy"` and added `[image.baremetal.knobs.executor] max_cbs = 4`. `nros run` then printed `matched after 3.2 s: [INFO] sensor_node: [1.021974] Publishing: 'Hello World: 1'`.
- 09:04 to 09:18. `ros2 topic echo` attempts. My first attempts were invalid (buffered output, a stale-log race and a pgrep that matched itself).
- 09:16:59. A clean attempt received nothing in 30 s.
- 09:18:14. `ros2 topic info -v` printed `Topic type hash: INVALID` and `Reliability: RELIABLE`.
- 09:18:56. A fresh scaffold in `~/a` with Jazzy sourced picked `ros_edition = "jazzy"`. The run failed again with E0080, which confirms stop 2.
- 09:19:49. After the knob was added, the run printed `the router exited (code 1) … Address already in use`. The cause was a router left over from my echo script, not the product.
- 09:20:06 to 09:20:10. The run printed `matched after 3.2 s`.
- 09:20:15 and 09:20:55. Echo still received nothing, and the type hash was still INVALID. This is stop 3.

### Task 2

- 09:21:51 to 09:22:18. I scaffolded `~/b/sensor_ws`, made the same edit and ran `nros build` (rc 0, about 25 s with a warm cache).
  - `diff -r src` showed only that `nros build` had added `std_msgs = { path = "../../generated/std_msgs" … }  # nros-managed` to `src/sensor_node/Cargo.toml`, plus a `metadata/` dir. So the build rewrites a tracked source file.
  - The image sha256s were a `55f96fa69ead…` and b `6fc6b81d7604…`.
  - The `objcopy` binaries were `996bc984…` and `a3449c13…`.
  - `size` was identical: text 811484, data 1984, bss 173188.
- 09:22:30 to 09:22:55. I moved `~/a/sensor_ws/build` away and rebuilt. The result was `55f96fa69ead…`, byte-identical to the first a build. This is stop 5.

### Task 3

- 09:23:14. `nros setup --offline-dirs` printed `~/.nros`, `~/.rustup` and `~/.cargo`. `install.sh --version 0.6.0-nros7 --dry-run` printed the URL `…/releases/download/nros-0.6.0-nros7/nros-linux-x86_64.tar.zst`.
- 09:23:23 to 09:24:22. The book's deb recipe produced 10 debs, 2 of them `ros-jazzy`. This is stop 6.
- 09:24:30 to 09:27:44. My full deb set: 534 debs, 206 MB.
- 09:27:38. I downloaded the asset and its `.sha256` (`16afd267…`, which matches) and `install.sh`.
- 09:27:49. `nros configure` in `~/a/sensor_ws` printed `up to date`.
- 09:28:33. I tarred the stores: `.nros` 3.4 GB, `.rustup` 662 MB, `.cargo` 137 MB, 4.35 GB in total.
- 09:28:49. Container `probe2-ind-off` started with `--network none`.
- 09:30:04. `apt-get install ./*.deb` failed (libssl3t64 and ca-certificates).
- 09:30:13 to 09:34:56. I added the three ssl and ca-certificates debs; the install succeeded, with `/opt/ros/jazzy` present.
- 09:35:05 to 09:35:34. `NROS_INSTALL_URL=file:///carry/nros-linux-x86_64.tar.zst sh install.sh` printed `installed 0.6.0-nros7`, offline.
- 09:35:48 to 09:37:18. Extracting the stores to the same paths and running `nros setup mps2-an385-baremetal --rmw zenoh` printed `all packages already present`. No network was needed.
- 09:37:24. `nros new` worked offline.
- 09:38:02. `nros run` without `NROS_OFFLINE` failed with `Could not resolve host: index.crates.io`. This is stop 7.
- 09:38:08 to 09:40:16. With `NROS_OFFLINE=1`, `nros run` printed `matched after 3.4 s: … Publishing: 'Hello World: 1'`. That was the first output line, offline, at T+44:25.

### Task 4

- 09:33:38 to 09:33:59. After `rustup component add llvm-tools` (online), `llvm-size -A` printed:
  - `.text 681296`, `.rodata 129728`, `.data 1984`, `.bss 173188`;
  - top statics: `HEAP` 132,816 B (b), `g_sessions` 17,072 B, `zpico_serial::port::PORTS` 4,128 B, `SMALL_PAYLOADS` 4,096 B, `MESSAGE_INFO_TABLE` 4,096 B and `EXECUTOR_BACKING` 3,824 B;
  - a boot message that suggested `arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE=1024 … 184/2048 bytes claimed`.
- 09:34. `cargo metadata` failed because jq was missing; the python listing gave 140 packages. Licenses include MIT/Apache, 0BSD (smoltcp, managed), Unlicense, BSL-1.0 (ryu) and BSD-3-Clause (encoding_rs).
- 09:34:43. `nros doctor --report` and `nros board show` ran; their content is in stops 8 and 12. The report also lists the pins `nros-toolchain.toml` (`version = "0.6.0-nros7"`, written by `nros build`) and `nros-sdk.lock`.
- Forks index: zenoh-pico 1.10.1, cyclonedds 0.10.5, lwip STABLE-2_2_1, threadx and netxduo v6.4.5.202504_rel, nuttx 12.13.0, libc 0.2, qemu v11.0.0 and ros2-rust-examples main. This image uses zenoh-pico, possibly mbedtls, and the qemu fork as a host tool.

### Task 5

- 09:40 to 09:44. I read Support status › Release support, Install options › Pinning a version and the Deploy to production checklist. No security, CVE, SBOM, LTS or EOL text exists in the book (grep found nothing). This is stop 13.

### Cleanup

- 09:44. I ran `docker rm -f probe2-ind probe2-ind-off` and deleted the 4.35 GB store tarball from the scratchpad.
