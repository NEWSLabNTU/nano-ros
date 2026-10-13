# Changelog

Assembled from `changelog.d/` fragments by [towncrier](https://github.com/twisted/towncrier).
Do not hand-edit below the marker — add a fragment instead (`just changelog-add`),
so two pull requests never edit the same region. See `changelog.d/README.md`.

<!-- towncrier release notes start -->

## 0.6.0 — 2026-10-14

### Fixed

- `CONFIG_NROS_SNTP_EPOCH` now works for single-executor Zephyr images and reaches the wall clock. Two gaps kept an acquired epoch from any node: only the tiered entries (`zephyr_run_tiers.c`, `entry_tiers.rs`) called `nros_zephyr_epoch_acquire_configured`, so an image entering through `ZephyrBoard::run_components` or `nros_board_rtos_run_components` never asked for one; and the Zephyr port's `nros_platform_time_now_ns` returned a hard 0, so `rclcpp::Clock().now()`, `nros_clock_get_now_ns(NROS_CLOCK_SYSTEM_TIME)` and `nros_core::Clock::system()` never saw the epoch even when it was there. Both entries now acquire it after network-up and before the first component, and the wall clock returns it (still 0 until one is acquired). ([#758](https://github.com/NEWSLabNTU/nano-ros/issues/758))
- The static pool inventory states a cost for every sizing knob — a byte figure, or where the bytes go and why no static figure exists — and `LARGE_PAYLOADS` is priced again (131,072 B at defaults) ([#815](https://github.com/NEWSLabNTU/nano-ros/issues/815))
- the issue ledger no longer conflicts when two agents file concurrently ([#884](https://github.com/NEWSLabNTU/nano-ros/issues/884))
- CI generates message bindings before the compile tier, so native::check can run at all ([#898](https://github.com/NEWSLabNTU/nano-ros/issues/898))
- NROS_ZPICO_DEBUG=1 builds the vendored zenoh-pico with DWARF, so a debugger can see inside it ([#899](https://github.com/NEWSLabNTU/nano-ros/issues/899))
- the artifact-identity gate resolves its build tree instead of naming a path nothing writes ([#901](https://github.com/NEWSLabNTU/nano-ros/issues/901))
- record that a zenoh-pico edit rebuilds nothing — zpico-sys watches 7 files out of the library ([#902](https://github.com/NEWSLabNTU/nano-ros/issues/902))
- zenoh sessions no longer drop and rebuild every ~20 s — the client lease now matches the ROS router keep-alive cadence ([#906](https://github.com/NEWSLabNTU/nano-ros/issues/906))
- codegen writes generated files atomically, so an interrupted run no longer leaves a zero-byte source file ([#920](https://github.com/NEWSLabNTU/nano-ros/issues/920))
- a lost peer no longer wedges the transport teardown — the connection is aborted rather than gracefully shut down ([#924](https://github.com/NEWSLabNTU/nano-ros/issues/924))
- The arm64 `arm-none-eabi-gcc` dist (`13.2-nros5`) now ships `libffi.so.7` and `libmpdec.so.2` beside its bundled CPython, so `import ctypes` and `import decimal` work inside `arm-none-eabi-gdb`. Issue 0932's residue note named two unimportable lib-dynload modules and was written from reading; a sweep of all 44 found FOUR, and the one it missed — `_ctypes`, which has no pure-Python fallback and is what a pretty-printer can reach — was the one that mattered. The OpenSSL 1.1 pair (`_ssl`, `_hashlib`) stays deliberately unbundled: a deprecated TLS stack inside a debugger is a cost with no user, and the index already declares `[prereq.libssl3]` for other tools. Both bundled libs top out at GLIBC_2.17, so the dist's floors are unchanged (re-measured, not carried). The build now IMPORTS every lib-dynload module inside the shipped gdb and fails when the unimportable set differs from a recorded baseline in either direction — a module that starts failing is a regression, one that starts working is a fix — so the note cannot drift again; the arm64 runner printed `lib-dynload 44 module(s), unimportable = [_hashlib _ssl] (baseline)`. x86_64 is untouched (static CPython, no C-API exports, so no extension can load on any host) and macos-arm64 rebuilt byte-identical ([#932](https://github.com/NEWSLabNTU/nano-ros/issues/932))
- the cyclonedds graph TU no longer fails to compile for freestanding C++ targets ([#942](https://github.com/NEWSLabNTU/nano-ros/issues/942))
- a missing cross Rust target is reported before a build starts, not twenty minutes into one ([#943](https://github.com/NEWSLabNTU/nano-ros/issues/943))
- the SDK index and config/rust-targets.txt can no longer drift apart; armv8r-none-eabihf was missing from the index ([#944](https://github.com/NEWSLabNTU/nano-ros/issues/944))
- The connect locator an embedded image dials now has ONE producer; the entry and node-register lanes keep their own (differing) defaults instead of drifting ([#946](https://github.com/NEWSLabNTU/nano-ros/issues/946))
- the NuttX fallback opaque storage was eight bytes short of the session struct it holds ([#954](https://github.com/NEWSLabNTU/nano-ros/issues/954))
- closing a zenoh session no longer waits out a keep-alive interval — teardown was 20 s after the lease change ([#959](https://github.com/NEWSLabNTU/nano-ros/issues/959))
- zenoh builds no longer recompile zpico-sys on every invocation ([#966](https://github.com/NEWSLabNTU/nano-ros/issues/966))
- requesting ROS domain 0 explicitly now resolves to domain 0 instead of the sentinel value 255 ([#972](https://github.com/NEWSLabNTU/nano-ros/issues/972))
- generated cyclonedds projects no longer pin the Cyclone domain to 0, which silently split it from CONFIG_NROS_DOMAIN_ID ([#974](https://github.com/NEWSLabNTU/nano-ros/issues/974))
- The QoS status-event interop cell can run. `build_qos_event_probe` asked the fixture table for a row with an `rmw-zenoh` cargo FEATURE, but that crate has no `[features]` table and its row is a plain `rmw = "zenoh"` — so every run died at "no [[fixture]] row … with Selector {…}" before reaching a peer, which is why `native-qos-event-rust-zenoh-r2n` had never produced a verdict. With the selector corrected the cell passes against a live ROS 2 host: the probe sees a positive `alive_count_change` from a stock `demo_nodes_cpp` talker, and the same probe with no talker reports no event and exits 4. ([#1003](https://github.com/NEWSLabNTU/nano-ros/issues/1003))
- Every Zephyr XRCE example boots and delivers again. `xrce_session_state_t` is one heap allocation out of the platform arena, and at the defaults the twelve C and Rust overlays carried it was 423,824 bytes against 65,536 — 62.9 % entity slots the image does not have, and 30.9 % reliable stream buffer sized at an MTU no conf could reach. The twelve overlays now state their entity caps the way the six C++ ones have since issue 1033, and `CONFIG_NROS_XRCE_CUSTOM_TRANSPORT_MTU` (defaulting to `CONFIG_NROS_XRCE_TRANSPORT_MTU`) makes the MTU of the only transport a Zephyr image links settable for the first time — the general knob binds `UCLIENT_UDP/TCP_TRANSPORT_MTU`, both of which sit behind a profile no Zephyr build defines. ([#1010](https://github.com/NEWSLabNTU/nano-ros/issues/1010))
- the six standalone zephyr C++ examples build again — phase-412 retired the `ENTITIES` declaration they had just gained, and their XRCE session caps are now stated where the derivation can no longer reach (listener: 54,928 bytes against a 65,536-byte heap, from an image that would not configure) ([#1033](https://github.com/NEWSLabNTU/nano-ros/issues/1033))
- An arena exhaustion now says WHERE as well as how much: `boot_report` stamps `Stage::RegisteringEntities` (declared, documented and decoded since phase-412, emitted by nothing), so a dump can tell "ran out during registration" from "never registered anything". A new cell takes one real exhaustion all the way to `scripts/read-boot-report.py` -- symbol lookup out of the ELF, dump length, positional decode, and a verdict naming the value to set -- and asserts the `nros_log` channel in the same failure; `check-boot-report-layout` now also holds the `Stage` ladder in step with the decoder's table ([#1036](https://github.com/NEWSLabNTU/nano-ros/issues/1036))
- Five C-API contracts behind adopted rcl names now behave the way upstream's do (phase-417 stage 3). `rcl_client_is_valid` / `rcl_service_is_valid` / `rcl_subscription_is_valid` and their name accessors accept every state the entity actually works in — a REGISTERED or POLLING client, a polling service or subscription — instead of `INITIALIZED` alone, so the upstream guard no longer rejects a live entity and `rcl_client_get_service_name` no longer hands NULL to a `printf("%s")`. `rcl_node_is_valid` consults the node's CONTEXT, so it reports false over a session `rclc_support_fini` has dropped rather than passing the guard into a use-after-free. `rcl_timer_fini` is idempotent, as `rcl/timer.h` promises for a NULL or already-invalid timer. `nros_log_severity_t` sits on rcutils's number line (`UNSET=0, DEBUG=10 … FATAL=50`) and is a transparent `int`-width mirror of the C enum, so a ported rcutils constant means what it says instead of arriving as an invalid discriminant. **BREAKING for C callers:** `nros_timer_get_time_until_next_call` now takes rcl's shape, `nros_ret_t (const nros_timer_t *, int64_t *)` — it drops the caller-supplied clock, reads the executor arena rather than a field no dispatch path updates (it answered `0` for every running timer), and reports overdue as a negative value. ([#1049](https://github.com/NEWSLabNTU/nano-ros/issues/1049))
- The declared QoS history depth reaches the compiler again: a contract's `qos: { depth: N }` on a subscriber endpoint is rendered per component and `NROS_SUBSCRIBE` static_asserts each call site against it, so a code/contract disagreement fails the BUILD. `nano_ros_node_register()` claims the include dir and a deferred render after `nano_ros_entry()` fills it, which is what lets a model-driven producer replace the retired `ENTITIES` without an extra configure pass. The table is keyed on the TOPIC a call site writes rather than on the endpoint ref the contract addresses it by, and a contract stating `depth: 0` is refused rather than rendered. ([#1084](https://github.com/NEWSLabNTU/nano-ros/issues/1084))
- an image whose interface dependencies are declared in more than one package now converges its derived size knobs inside ONE `west build` -- the re-configure arm used to be per producer CALL, so the second `nros_find_interfaces()` settled the date the first had armed and the build compiled at the previous pass's sizes ([#1119](https://github.com/NEWSLabNTU/nano-ros/issues/1119))
- The two CycloneDDS interop cells that are shell rather than Rust --
  `ros2_pubsub_e2e.sh` and `ros2_srv_e2e.sh` -- now take their bus config from one
  shared `nros_export_cyclone_config` helper, which confines it to loopback the
  way `nros_tests::dds_isolation` has confined every other DDS lane since issue
  1009. Each script previously wrote its own copy of a config pinned to a real
  ethernet interface with multicast left on, so a DDS participant anywhere on the
  LAN was a peer of these cells and of no others: measured on one host, a nano-ros
  subscriber on the old config took a foreign `/chatter` publisher's sample 5 of 5
  times and 0 of 5 on the new one, with an unroutable-address negative control
  failing to prove the file is read at all.

  `ros2_pubsub_e2e.sh` also stops betting fixed second counts on the startup of a
  Python `ros2` CLI. Its sub-cases wait on a deadline and stop the moment the
  payload lands, which makes the happy path roughly 3 s instead of 12 s, and it
  keeps the publisher's exit status and every process's stderr instead of sending
  them to `/dev/null` -- previously a publisher that never opened a session and a
  delivery that never happened printed the same line. With the `ros2` CLI's
  startup slowed by a shim: at 3 s the old script failed 2 of 8 runs, at 6 s and
  at 10 s it failed 8 of 8; the new one passes 8 of 8 at every delay. It also
  passes `--no-daemon` to `ros2 topic echo`, which stops the cell leaking one
  `ros2` daemon per run (5 of 5 before, 0 of 5 after), each of which held a
  CycloneDDS participant for about two hours.

  `check-dds-isolation-symmetry` now covers shell cells too -- its scan was
  `packages/testing/nros-tests/**/*.rs` only, which is why these two were exempt
  by accident of language. ([#1139](https://github.com/NEWSLabNTU/nano-ros/issues/1139))
- `just mem-report` files executor and component storage under `[executor storage]` / `[component storage]` on C, C++ and Rust images (no longer under a Rust crate that shares a C++ namespace's name), and prices the C/C++ executor storage as `N x NROS_CPP_EXECUTOR_STORAGE_SIZE` from the build's own sizes header ([#1147](https://github.com/NEWSLabNTU/nano-ros/issues/1147))
- A Zephyr image that lowers `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` to pay for the
  executor's `.bss` backing now STATES the reservation with
  `CONFIG_NROS_EXECUTOR_BACKING_U64S` instead of copying a size out of `nm`. The
  reservation is `8 x` that count on every target, so the arena's lowering is exact
  by construction rather than a literal that goes stale the next time an executor
  knob moves — and it is correct on every board at once, which a single copied
  number could not be (the derived size is 87,256 B on mps2_an385 and 88,328 B on
  native_sim/native/64 for the same configuration). A stated count below what the
  image's executor needs is a compile error naming the knob;
  `check-executor-backing-arena-pairing` holds the conf's arithmetic together. ([#1171](https://github.com/NEWSLabNTU/nano-ros/issues/1171))
- The three `parity_test` whole-package walks measured their parse/codegen failures and then passed; they now hold the result to a committed expected-failure ledger, exactly and in both directions, and a walk over the bundled interface sources gives that ledger a verdict on hosts with no ROS ([#1176](https://github.com/NEWSLabNTU/nano-ros/issues/1176))
- `just mem-report --baseline` matches symbols across builds whose `.llvm.<hash>` / `.N` suffixes differ, annotates every row (`(+N)`, `(=)`, `(new)`), and lists symbols that are new or gone instead of printing them as unchanged ([#1180](https://github.com/NEWSLabNTU/nano-ros/issues/1180))
- `check-cpp-capability-layout` can now measure the type it was filed over.
  Issue 1204 recorded that the gate forced capability macros ON against a
  baseline where they were already on, so it compared the baseline against itself
  seven times, and that `rclcpp::Node` could not be measured any other way
  because the whole class sat inside `#if defined(NROS_CPP_HAS_SHARED_PTR) &&
  ...` — an `#ifdef` on that macro *inside* that block is tautological.

  phase-427 W1-W3/W5 removed the guard: `rclcpp::Node` is `::nros::Node`, one
  class declared unconditionally, with the hosted state behind a single lazily
  allocated `detail::NodeHostedBase*` and the `shared_ptr`-returning factories as
  additive overloads beside unconditional out-ref forms. phase-438 W3/W4 is what
  MEASURES that outcome: the gate gains a third arm — hosted `-std=c++17`
  **without** `-DNROS_CPP_STD`, which isolates the opt-in flag from the toolchain,
  because the freestanding arm varies `-std`, `-nostdinc++` and the shim all at
  once — and `rclcpp::Node`'s exemption is gone. The arm covers all 90 subjects
  issue 1225's derivation produces, and the derivation asks for the flag itself,
  so a subject that only exists in the porting surface cannot slip out of the
  gate; the one that does, `rclcpp::NodeOptions`, is declared `std-only` in
  `.config/cpp-capability-layout-baseline.txt`.

  Measured: `sizeof(rclcpp::Node)` is **200** in all three arms — hosted with the
  flag, hosted without it, and `-std=c++14 -ffreestanding -nostdinc++` against the
  ThreadX shim.

  A second check states the half `sizeof` cannot: the `cpp` lane now compiles
  `packages/api/nros-cpp/tests/compile/rclcpp_node_freestanding_surface.cpp`,
  which instantiates every unconditional factory and derives from the class, in
  both no-flag arms. Deleting a method moves no layout, so the capability gate
  stays green on that mutation while the probe fails — the two are complementary,
  and neither alone states the contract. ([#1204](https://github.com/NEWSLabNTU/nano-ros/issues/1204))
- `nros build` now builds every image it planned. `--all`, several positional images and a `[system] default_images` naming several each produce N plans, and the driver loop execed inside itself — `exec` never returns, so image 1 was built, images 2..N were never reached, and the command exited 0 with image 1's verdict and one announcement line on the terminal (measured on `examples/workspaces/rust`: two images asked for, one artifact, `rc=0`). RFC-0065 D1's `exec` is spent on the LAST plan, so a single-image build is unchanged in every respect; the earlier plans run as children with **inherited** stdio — no pipe anywhere, so diagnostics, colour and Ctrl-C behave as before — and a multi-image run prefixes each announcement `[i/N]`, which is the only progress report available once the last plan replaces the process. ([#1206](https://github.com/NEWSLabNTU/nano-ros/issues/1206))
- Ten shipped crates now carry `#![forbid(unsafe_code)]`, where before not one crate in `packages/{core,api,platform,boards,rmw,drivers,tooling}` denied or forbade it — so `nros-serdes` (the CDR serializer, ~5 000 LOC, the crate the Kani/Verus harnesses assume) was held at zero unsafe by nothing but habit. Re-measured before acting: thirteen crates are at literal zero, not the three the issue found, and the ten that took the lint are the ones whose ROLE implies the property — three pure data-transformation core crates, five host-side build-script helper libraries, two board crates that carry no hardware access. Three were deliberately left alone: two FreeRTOS board crates whose own docs say a Rust console writer is still to come, and a `-sys` crate that is thirteen lines of doc comment. `forbid` is the gate rather than a census script for this population: it cannot be locally overridden (it outranks a later `allow`), it fails in the author's own `cargo check`, it has no authored crate list to drift out of step with the rule, and it cannot be satisfied by a spelling change. Each one's enforcement was MEASURED with a planted `unsafe` block, ten of ten; `just check no-std` plus explicit `thumbv7m-none-eabi` / `riscv32imc-unknown-none-elf` checks cover the bare-metal half. ([#1221](https://github.com/NEWSLabNTU/nano-ros/issues/1221))
- `sizeof(nros::Timer)` no longer follows a capability macro, and neither do
  `nros::GuardCondition`, `nros::NodeWithTimers<N>` or the `rclcpp::Timer` /
  `rclcpp::TimerBase` spellings a ported file writes. All five held one member
  between them — an `#ifdef NROS_CPP_STD`
  `std::unique_ptr<std::function<void()>> closure_` — so a `Timer` was 24 bytes
  or 32 depending on a flag that one module of an image may set on its own, which
  `examples/px4/cpp/bridge/` does deliberately.

  The member is unconditional now, spelled `void*`, pointing at a block whose
  head is `detail::HostedBlockBase`: the block carries its own
  `void (*destroy)(void*)`, so `~Timer()` — compiled in both configurations —
  frees it without naming a hosted type. That is the shape `rclcpp::Node::hosted_`
  already used; `nros/hosted_block.hpp` is now the one place it is written, and
  `node.hpp` uses it rather than its own copy.

  The decision behind it, from issue 1225: state hides behind a pointer only when
  it EXCEEDS a pointer. These types differed by exactly 8 bytes, which is a
  pointer, so the unconditional member costs the freestanding arm exactly what an
  indirection would have cost it, with one stable layout and no dereference on
  the hot path.

  The five `diverges` rows are gone from
  `.config/cpp-capability-layout-baseline.txt`. The gate is the acceptance rather
  than the paperwork: a baseline row naming a subject that no longer diverges is
  a failure, so the file could not have been left stale. ([#1225](https://github.com/NEWSLabNTU/nano-ros/issues/1225))
- The executor arena now prices each subscription at the QoS depth that endpoint
  DECLARED, instead of billing every one at the ROS default of 10. On the
  safety-island image that is 129,048 bytes of arena against 50,640, and the
  image links again -- DTCM 98,040 B, 74.80% of 128 KB.

  Nothing about the running code changed. The declarations were always there:
  its eleven subscriptions are created with `QoS(1)` and always have been. What
  changed is that the build is now told, so it stops sizing eleven-slot rings for
  two-slot queues. ([#1227](https://github.com/NEWSLabNTU/nano-ros/issues/1227))
- The entity-inventory fragment a Zephyr configure hashes no longer carries content that depends on WHICH COMPOSER wrote it, so `nros build`'s resolve phase (stage 3.5) and the mid-configure producer agree byte for byte and the producer stops arming a re-configure over provenance. Two renderings moved out of `to_cmake` and into `nros/entity_inventory.json` and `resolved.toml`'s `[provenance]`, where a byte comparison cannot reach them: `NROS_ENTITY_INVENTORY_SOURCE`, and the per-component provenance line's PACKAGE — the model states a node FQN (`/talker`) where the merge states an ament package (`talker_pkg`), which is the half issue 1228 had not seen because its unit test built both composers with the same package. Measured on `demo_bringup:zephyr` (`native_sim/native/64`, Zephyr 3.7), clean build dir: the arms in pass 1 go from two to one and the seed and the producer now `diff` empty, with `check-knob-delivery` green on both sides and the delivered knob values and the image's `.config` byte-identical. The pass count itself is unchanged at one re-configure, because the surviving message-bound arm fires in the same pass — that link, and the deletion of `nros_reconfigure_settle` it gates, are issue 1252. ([#1228](https://github.com/NEWSLabNTU/nano-ros/issues/1228))
- `check-box-sync-covers-tracked-source` now sweeps every repository the ROS-box mirror copies — the superproject, every submodule and every west project — instead of the superproject's index alone, which is how a `target/` exclusion silently ate Zephyr's I²C target-mode source while the gate stayed green; the widened sweep immediately found a second live case in `PX4-Autopilot/boards/modalai/voxl2/target/`, both are re-included, and a tree the sweep cannot see is now reported as NOT VERIFIED by name rather than passing as OK ([#1229](https://github.com/NEWSLabNTU/nano-ros/issues/1229))
- Every published `NROS_DERIVED_*` fact now states what each of the three delivery roads (resolver / sidecar / declared) does with it: `check-declared-fact-carriers`'s new `FACT_DISPOSITION` requires a carried-name list, or a deliberate omission QUOTING a comment that already exists in the tree, or a reported `OpenGap` carrying a tracked issue id — and `check-knob-delivery`'s `DERIVED_PAIRS` stopped being 1:1 (one fact feeds two resolved knobs twice over, so a value that arrived at one consumer and was dropped at the other used to be green) and is now cross-checked against the `_nros_resolve_derivable_knob` call sites in both directions on every run ([#1233](https://github.com/NEWSLabNTU/nano-ros/issues/1233))
- The `find_package(rclcpp)` compat entry point works again, and now matches the
  `ament_auto_*` one. `cmake/compat/stubs/Findrclcpp.cmake` still force-included
  `nros/rclcpp_compat.hpp`, a header deleted in phase-417 when the `rclcpp::`
  spellings moved into `<nros/nros.hpp>` itself, so every C++ TU compiled against
  the stub died with `fatal error: nros/rclcpp_compat.hpp: No such file or
  directory` before reading a line of its own source. It also never set
  `NROS_CPP_STD`, which phase-438 W2 made the way to ask for the std-flavoured
  `rclcpp::` surface — `ament_target_dependencies` links the stub's INTERFACE
  target without going through the per-target helper that does set it. Both are
  fixed; a stock `find_package(rclcpp)` + `ament_target_dependencies` package
  compiles again. ([#1239](https://github.com/NEWSLabNTU/nano-ros/issues/1239))
- `nros setup --tool zephyr-sdk-1-0-1` no longer reports success over an SDK with no toolchains — a `[tool.*]` may declare a `post_install` step that completes the install (recorded in `.nros-provenance`, so a re-run is a no-op and a failed one resumes without re-downloading) and a `subdir` naming the tarball's own top-level directory, which `nros sdk-path` now answers and `smoke` probes resolve against ([#1259](https://github.com/NEWSLabNTU/nano-ros/issues/1259))
- `nros` compiles again with `param-services`. Two callers of `Executor::declare_parameter` discarded the result that phase-428 W6 made `#[must_use]`, so every image declaring `param_services` failed to build. A refused launch `<param>` now refuses the boot and names the parameter, and a refused component parameter returns the new `NodeDeclError::ParameterRejected`. On Zephyr the capability now names `alloc` as well, so a zenoh image declaring `param_services` builds instead of stopping at nros-node's `compile_error!`. `compile-smoke` now checks `nros-c` / `nros-cpp` with `param-services`, so the host path is compiled on every pull request. ([#1260](https://github.com/NEWSLabNTU/nano-ros/issues/1260))
- `nros setup` now records every tool it was asked for in `nros-sdk.lock`, and puts that lock in the project rather than in whatever directory you ran from. A tool already in the shared store planned as `present (skip)` and was written nowhere, so the second project to ask for an SDK committed a lock naming one toolchain while needing two — and left `nros store gc` believing nothing pinned the other. The lock path was a relative `nros-sdk.lock`, so it followed the cwd; it is now anchored to the nearest ancestor carrying a pin file (`nros-sdk.lock`, `nros-sdk-index.toml`, `nros-toolchain.toml`), which is the same walk `nros store gc` reads them back with. `nros setup --tool` prints which file it wrote. ([#1262](https://github.com/NEWSLabNTU/nano-ros/issues/1262))
- Board facts can reach Zephyr images again. `nros_resolve_board_facts` looked the CLI up under one of its two cache names, cached every failure in the CMake cache permanently, and never passed `--nano-ros-path`, so every Zephyr configure reported "no nros CLI" and a build dir that once failed could never recover. It now asks the shared `nros_resolve_cli`, remembers a failure for one configure only, and names the checkout it belongs to. ([#1263](https://github.com/NEWSLabNTU/nano-ros/issues/1263))
- a manifest parser that fails to even start is reported as a parser failure, not as an unknown platform or fixture id ([#1264](https://github.com/NEWSLabNTU/nano-ros/issues/1264))
- `nros new --platform baremetal|esp32` projects now build, and `nros sync` now
  derives their pool sizes instead of leaving them at the crate defaults. The
  template puts everything only the firmware image needs (board crate, HAL,
  runtime, panic handler) under `[target.'cfg(target_os = "none")'.dependencies]`,
  so the node in `lib.rs` also builds for the host, where `nros sync` probes what
  it creates. Two template bugs that kept every such project from compiling are
  fixed with it: a duplicate panic handler (`nros::main!(panic = "own")` now) and
  an esp32 `main.rs` naming an `esp_hal::esp_app_desc!` that does not exist.

  If your own embedded project's `lib.rs` is probed and you see
  `<component>.json.unprobeable` after `nros sync`, apply the same split: a
  target-only crate in plain `[dependencies]` is compiled for the host too, and
  the probe fails on it. The esp32-c3 and mps2 bare-metal examples follow this
  shape and no longer state `entities` in `system.toml`. ([#1265](https://github.com/NEWSLabNTU/nano-ros/issues/1265))
- Parameter services on Cyclone are verified against a live ROS 2 peer, not only against our own tests. `ros2 service list`, `ros2 param list/get/set` were run against a two-node Cyclone image with `param_services`: all twelve `rcl_interfaces` services are on the wire (six per node), each node answers with its own value for a parameter name they share, and a write to one leaves the other alone. The two-node fixture gains the RMW feature axis it needed to be pointed at Cyclone at all — it was pinned to zenoh, the one backend on which a parameter service cannot fail on type grounds — and the run is now an interop cell (`native-params-per-node-rust-cyclone`) with a recorded verdict, so it stays checked. ([#1268](https://github.com/NEWSLabNTU/nano-ros/issues/1268))
- A parameter service request that gets no reply (a reply larger than `NROS_PARAM_SERVICE_BUFFER_SIZE`, a request past the wire caps, a transport failure) is now logged once per service with the cap and the knob, instead of being dropped silently. A failure to create the parameter services is logged once instead of retried in silence. ([#1271](https://github.com/NEWSLabNTU/nano-ros/issues/1271))
- Launch parameters for every node of a multi-node entry are seeded on that node, not all on the executor's primary node, so two nodes that set the same parameter name each read their own value. `nros_cpp_declare_param` takes the node's index as a new second argument (C ABI change), the generated C and C++ entries pass it, and the shim refuses an index that is not the next node the executor builds. `nros::main!` seeds each node's values as that node is built, and `RuntimeCtx::apply_param_services` no longer takes a parameter list. ([#1272](https://github.com/NEWSLabNTU/nano-ros/issues/1272))
- A fresh Zephyr workspace no longer fetches `hal_espressif`, which is 275 MB nothing in the tree consumed — the last of the four vendor HALs issue 1275 measured as unused, and the one held back while the strategy question behind it was open. That question is now settled: **bare-metal esp-hal is the esp32 path, and Zephyr-hosted esp32 is not supported for now.** Nothing that worked stops working — no board, conf, overlay or fixture row ever named an Espressif board through Zephyr, and the Zephyr SDK we provision has never carried an `xtensa-espressif_*` toolchain to link one with — so the dead `CONFIG_SOC_SERIES_ESP32C3` Rust-target branch went with it. The decision is reversible; re-adding the module means re-adding a board and a toolchain beside it. Note that `west update` never prunes, so an existing workspace keeps the 275 MB until it is re-fetched. ([#1282](https://github.com/NEWSLabNTU/nano-ros/issues/1282))
- `nros::arena_size_for(cbs)` asks the model on an image that declares its entities instead of scaling the derived arena by `cbs / MAX_CBS`, a ratio that stopped meaning anything when the arena became a per-kind sum. The bench's only executor was getting half of what its own declaration needed ([#1290](https://github.com/NEWSLabNTU/nano-ros/issues/1290))
- A failed `rclcpp::Node::create_*` and a refused `--ros-args` token now say why on an RTOS console. Each refusal used to be one log record of 870–1180 bytes, which `nros_log` dropped for not fitting its buffer, so the process aborted after printing only an ellipsis. Each is now two short records: the call and its value, then the reason. The fixed texts behind `NROS_RCLCPP_REFUSE_INIT_ARGV` and `NROS_RCLCPP_ABORT_FAILED_CREATE` are shorter to match. ([#1303](https://github.com/NEWSLabNTU/nano-ros/issues/1303))
- An installed `nros` now provisions and builds with no checkout: a release records its submodule pins in the SDK root it ships, `nros setup <board>` installs a Rust toolchain when the host has none, and configure uses the Cyclone DDS `nros setup` provisioned. The scaffolds no longer advertise `NROS_REPO_DIR` or `-DNROS_REPO_DIR` to a reader who has neither, and the installed probe exercises the book's own `nros sync` + `nros build` flow again — phase-445 W5 had dropped first-project.md's `probe=` tags, so the track had been extracting no build step at all. ([#1304](https://github.com/NEWSLabNTU/nano-ros/issues/1304))
- Every single-package example on a supported board now gets its board crate,
  RMW backend and runtime crates from the generated `<pkg>_nros_selection`
  package, so switching board is the one `[image.*] board` line. The last
  classes moved: the copy-out templates' Rust packages (`nros sync` in a
  workspace now writes a member package's selection before its first cargo
  call), the bare-metal MPS2-AN385 C/C++ applications on a cargo link root and
  the native Rust examples with a hand-written `main` (their `main.rs` reaches
  the board through the selection package; each now has a `system.toml`).
  The in-tree test programs, benches and harness fixtures under
  `packages/testing/` followed: each names its board in `system.toml` (or, for a
  workspace entry, in the bringup image that claims it, which `nros sync` now
  also writes a selection from), and a board-specific program turns on a board
  feature through its selection dependency (`features = ["docker"]`).
  An image can leave the host board's census and profile producers out of its
  selection with `[image.<id>] producers = false` (default: on); issue 1340's
  memory-measurement witness does, and measures as before. ([#1305](https://github.com/NEWSLabNTU/nano-ros/issues/1305))
- the Cyclone RMW test suite builds from a clean checkout again — its generated descriptors had no owning target, so a parallel make ran idlc up to eleven times at once into the same files ([#1311](https://github.com/NEWSLabNTU/nano-ros/issues/1311))
- A Zephyr C++ component compiles whatever its translation unit included first. `component.hpp` now includes `<new>` itself on Zephyr, so its placement-new shim is always in force; before, the shim was skipped when nothing had already pulled Zephyr's stub `<new>` in, and `NROS_COMPONENT` failed with "no matching function for call to `operator new(sizetype, void*&)`". ([#1317](https://github.com/NEWSLabNTU/nano-ros/issues/1317))
- A workspace that keeps its out-of-source build trees beside `src/` is no longer walked through them. The prereq scan's two `package.xml` walks pruned `build` and `target-*` but not `build-*`, so every `build-<name>` directory a `west build -d` or `cmake -B` had left behind was scanned in full: measured on a tree with 85 of them, one `nros image-facts` opened 362,782 directories, 98.7% of every open it made, to read a single file. Both walks now share one pruning predicate, which also honours the ament ignore markers the pkg-index walk already honoured. ([#1337](https://github.com/NEWSLabNTU/nano-ros/issues/1337))
- The two zenoh action interop cells met a real ROS 2 peer for the first time, and two findings came out of it. The nano-ros side of `native-action-rust-zenoh-n2r` works: the action client drives a stock `examples_rclcpp_minimal_action_server` over `rmw_zenoh_cpp` to Fibonacci(10). The other direction does not run at all — a zenoh action SERVER now fails to start, because its `/fibonacci/_action/status` publisher asks for the TRANSIENT_LOCAL durability `rcl_action_qos_profile_status_default` requires and the shim began refusing that on 2026-09-12 (issue 1341, with the 2026-09-08 build of the same fixture measured still running). Separately, a fixed-domain zenoh cell inherits whatever ros2cli daemon domain 0 already holds, which makes the `ros2 action list` discovery gate unmeetable for a reason that is not about nano-ros (issue 1342). ([#1341](https://github.com/NEWSLabNTU/nano-ros/issues/1341))
- esp32 images compile again — the zenoh QoS report-once latch used a `core` atomic with no `swap` on riscv32imc ([#1344](https://github.com/NEWSLabNTU/nano-ros/issues/1344))
- `check-stack-floor` stopped failing every esp32 fixture row on its own coverage assertion ([#1346](https://github.com/NEWSLabNTU/nano-ros/issues/1346))
- A Zephyr zenoh image that is built to receive more than zenoh-pico can reassemble now fails the configure instead of dropping those samples at run time. `NROS_SUBSCRIBER_BUFFER_SIZE` and `ZPICO_SUBSCRIBER_LARGE_SIZE` have been derived from the image's message bounds since phase-403, while `NROS_FRAG_MAX_SIZE` stayed a flat Kconfig `default 2048` that nothing compared them to -- and a fragmented sample that reassembles past it is dropped inside the transport on a success return, with the `_Z_INFO` beside the drop compiled out, so it reaches no callback and raises no error and the subscription simply looks like one that never fires. The build now names both numbers and the knob to raise. The ceiling it checks against is the larger of `ZPICO_FRAG_MAX_SIZE` and `ZPICO_BATCH_UNICAST_SIZE`, because a sample that fits one negotiated batch arrives whole and is never defragmented; on the embedded defaults it is the 2048-byte frag ceiling that binds. `NROS_FRAG_MAX_SIZE` is deliberately still not derived: fragmentation is a property of the transport and the link rather than of the message, and its useful upper end is the path MTU, so the build states the disagreement and leaves the value to whoever owns the board's RAM budget. ([#1368](https://github.com/NEWSLabNTU/nano-ros/issues/1368))
- A freshly created worktree no longer dead-ends at the pre-push gates. `git worktree add` populates no submodules, and two of the 324 fast gates read one: `capability-conditionals` needs zenoh-pico's platform dispatch header, `xrce-vendored-versions` needs both vendored XRCE `CMakeLists.txt`. The new `just setup-worktree` checks out those three submodules and only those three - not the 20 that a bare `git submodule update --init` would fetch - and both gates now name it in their failure text. The XRCE gate used to advise `nros setup --source micro-cdr --source micro-xrce-dds-client`, which cannot run where that failure happens: a fresh worktree has no `target/`, so the `nros` on PATH belongs to another checkout and `setup` refuses a foreign binary. ([#1373](https://github.com/NEWSLabNTU/nano-ros/issues/1373))
- Zephyr: a `west build` no longer inherits another checkout's `nros` module. Every
  configuring build now names its own checkout (`scripts/lib/zephyr-module.sh`), the
  derived `NROS_REPO_DIR` is FORCEd so a build directory cannot keep a foreign one in
  its cache, and an application and module belonging to different nano-ros checkouts
  are refused at configure time instead of linking a half-and-half image. ([#1379](https://github.com/NEWSLabNTU/nano-ros/issues/1379))
- `nros::main!()` no longer emits `std::` paths into a `#![no_std]` entry, and a
  single-package leaf is buildable by hand again: `nros sync` + a plain
  `cargo build --release` inside the leaf now picks up the board's triple and link
  group from the generated `build/<image>/nros-cargo.toml`, which cargo only ever
  saw through a `--config` flag the fixture lane passed. ([#1381](https://github.com/NEWSLabNTU/nano-ros/issues/1381))
- The Zephyr `rust/action-server` example now reports what it actually did. It says `Goal succeeded` only when `complete_goal` returned `Ok` and logs the error otherwise — printing the banner over a discarded `Result` is the same shape issue 0902 measured from the client side, where a goal is accepted, executed and then never answered while the transcript reads as a success. It also reports its zenoh reply-slot refusal count (`reply-slot: refusals=<n>`) on a cadence, so the one number that separates an exhausted reply table from a slow image is readable from the image itself rather than inferred from the client; a build with no zenoh shim prints `n/a no-zenoh-shim` instead of a misleading zero. ([#1387](https://github.com/NEWSLabNTU/nano-ros/issues/1387))
- `check-leaf-lockfiles` no longer reads the central `nros-patch.toml` as proof
  that the tree is provisioned. `nros sync` writes patch tables and `generated/`
  trees; it does not fetch submodules, so a fully synced tree can still have none
  checked out. A leaf whose `[patch.crates-io]` names a DECLARED submodule with no
  `Cargo.toml` is now classified as a provisioning gap — recorded through the
  `nros_check_skip` ledger, reported with `git submodule update --init <path>` —
  instead of being called "genuinely unresolvable … their patch tables or
  `generated/` trees are wrong" and sent to a command that cannot help. A leaf
  that fails for any other reason on a provisioned tree still fails hard. ([#1398](https://github.com/NEWSLabNTU/nano-ros/issues/1398))
- `[tool.play_launch_parser].source.ref` and the `packages/cli/third-party/play_launch` gitlink are two pins on one component, and three documents claimed they "cannot disagree" while nothing measured it — they had drifted ~40 commits (`838ce948` vs `07f0461e`). All three claims are corrected (the index, `just/workspace.just`, and `nano-ros-sdk`'s `build-play_launch_parser.sh`) and replaced by `check-play-launch-parser-ref`, which measures what is actually required: `upstream` and `source.ref` name the same commit, `source.ref` is an ancestor-or-equal of the gitlink (the index may LAG the submodule, never lead it), and while they differ the index carries a `# ref-lag:` reason that must be deleted when they converge. NOT an equality gate: the two are legitimately unequal because issue 0897 moved pyo3 out of the CLI crate, so at the gitlink `.launch.py` hard-errors and — measured, and worse — a `$(eval '1 + 1')` written into an `<arg default=…>` exits 0 emitting the substitution UNEXPANDED (a DIRECT `$(eval …)` hard-errors like `.launch.py`; this sentence said the silence was universal until the 2026-09-27 re-measurement below). Ancestry over a SHALLOW clone is read as "cannot tell" rather than "false": a `--depth 1` submodule reports a genuinely related pair as unrelated, measured here

  Correcting this issue's own first draft, which asserted unmeasured that `838ce948` was the newest Python-capable commit: pyo3 left in `f7f6d2cf`, so the newest is its parent `27b6749b` — 54 commits NEWER than the pin, built and run here (`libpython3.10.so.1.0`, `/from_python`, `/from_eval_2`), with a further 54 to the gitlink. The gap splits: `838ce948`→`27b6749b` is a SAFE bump blocked only on an acceptance nobody has run (a `0.1.0-nros2` re-cut plus `phase212_l6_launch_synth::*`), and only `27b6749b`→`07f0461e` is blocked on upstream

  The safe half then landed in the same PR: `source.ref`/`upstream` moved to `27b6749b` (= `f7f6d2cf^`, the newest Python-capable ref, 54 commits forward) and the dist was re-cut as `play_launch_parser-0.1.0-nros2`, with sha256, floors, `DT_NEEDED`-derived `system` and the `smoke` `expect` all re-measured off the new artifacts rather than carried. The acceptance `just/workspace.just` named — `phase212_l6_launch_synth::*` — turned out to be a sixth stale claim (issue 0381 rewrote those tests to synthesise in-process, so they never shell to the parser); the one that actually binds it is `nav2_compat`'s `n11_launch_xml_ros2_compat_smoke`, whose build-stage fixture drives codegen with `play_launch_parser` on PATH — rebuilt against the new parser, 11/11 green. #1413 stays open for the remaining 54 commits, which need an install target that stages `libplay_launch_parser_pyexec.so` beside the binary

  Then measured again, because the next step asked for is `source.ref` DELETED and the commit derived from the gitlink: it cannot be. `nros setup --tool` fetches the index over HTTPS and source-builds by cloning `source.git` into the SDK store, on a user's host with no nano-ros checkout — so a `source.submodule = "<path>"` field names a directory that is not there, and the same boundary stops `nano-ros-sdk`'s `build-play_launch_parser.sh` one layer up. Nor are the two a duplicated fact: issue 1454 measured the store binary running ZERO times in any build path, so the gitlink is ALREADY the sole spelling of which parser the BUILD consumes (via `nros-launch-resolve`, stamped into `.compile-ok`), while `source.ref` names a standalone CLI under a different constraint. `upstream` stays for the same reason: 25 tools declare it and `build-tool.yml` reads it for all of them. The gate is unchanged. The `$(eval …)` blocker was re-measured too, and is half the shape this issue drew — upstream `caab6fbc` (an ANCESTOR of the `07f0461e` the original table was measured at) made the DIRECT path exit 1 with a named diagnostic, which is this issue's own third remedy taken three weeks before it was filed; only the `<arg default=…>` path is still silent, and that is the one real launch files use. Both commit counts are DELETED rather than updated — each went stale within days, and the distance is one `git rev-list --count` away. Seventh and eighth stale claims retired: `nano-ros-sdk`'s script named `838ce948` as the newest Python-capable ref and as what the dist is cut from (it is `27b6749b`, since `0.1.0-nros2`), and the gate's own docstring repeated the universal-silence claim ([#1413](https://github.com/NEWSLabNTU/nano-ros/issues/1413))
- A refused resolve leaves no SystemModel any consumer can trust. When `nros sync` is refused (a contract the resolver rejects), the previous `system_model.yaml` is moved aside to `system_model.refused-<utc-stamp>.yaml` and a `system_model.refused` marker names the check and the input that changed; `nros model-path`, `ws entity-inventory`, `codegen-system` and `codegen entry` all refuse the model, naming the marker, until the next successful sync. Every consumer now verifies the model's provenance (resolver pin, recorded input hashes, and a launch-tree input the model never recorded) through one gate, not only sync. ([#1420](https://github.com/NEWSLabNTU/nano-ros/issues/1420))
- A contract that declares `params:` on some nodes and not others now stops the configure (CMake road: `NanoRosEntityFacts.cmake`; a cargo road that forwards `NROS_PARAM_DECLARATION_STATUS`: `nros-params`' build script) quoting the entity inventory's reason and the silent nodes, instead of sizing the parameter store from the crate defaults with no build-time line; `absent` (no node declares) and `declared` are unchanged ([#1421](https://github.com/NEWSLabNTU/nano-ros/issues/1421))
- The Zephyr bake (`nros_system_generate`) refuses the configure when `system.toml`'s `domain_id` and the image's `CONFIG_NROS_DOMAIN_ID` (or `CONFIG_NROS_CYCLONE_DOMAIN_ID`, when set) disagree, naming all three values; the precedence is unchanged (Kconfig is what the image bakes, RFC-0049) and only the silent disagreement is removed ([#1423](https://github.com/NEWSLabNTU/nano-ros/issues/1423))
- A C or C++ workspace whose model describes no wiring builds again. The cmake entity-facts emitter composed `NROS_DECLARED_TL_PUBLISHERS=` with no value, which is a fourth answer on a carrier whose contract has three (absent, `refused`, a count), and the zpico build script panicked on it — taking every template a user copies out that builds a workspace with it. The cause is a cmake reading trap: `get_property()` leaves its output variable UNDEFINED when the property was never set, so `if(NOT _v STREQUAL "")` compares the variable's NAME against `""`, never matches, and the guard meant to skip an absent fact emits a valueless one instead. All three emitter sites now read through `_nros_entity_fact`, which always defines its output; every `NROS_DECLARED_*` reader is total on an empty value (absent, with a `cargo:warning`, because a build script does not own its environment and `0` keeps its own spelling); and `check-declared-fact-carriers` gains a fourth rule that refuses the idiom outright, on the fast lane. ([#1429](https://github.com/NEWSLabNTU/nano-ros/issues/1429))
- `just check scaffold-builds` no longer reports the one scaffold that builds fine
  as a vacuous pass. Its artifact locator searched `<leaf>/target` and
  `<leaf>/build` at depth 2, while RFC-0098 D1 puts a cargo PROJECT's binary at
  `build/<image>/target/<profile>/<name>` — four levels down — so `rust_project`
  failed with "the build exited 0 but compiled none of the emitted source" about a
  build whose log says `Compiling rust_project` / `Finished`. The locator now
  READS the target directory out of the generated `build/<image>/nros-cargo.toml`
  instead of guessing it, checks an enumerated set of exact paths under it rather
  than widening a `find`, and requires every artifact to be newer than the build —
  so the fix cannot be answered by a leftover artifact or by another package's
  binary. Seven locator controls run offline (`--locator-self-test`, also part of
  `--self-test`) and four mutants of the locator are each caught by a different
  one. The artifact COUNT is no longer taken from a piped `grep -c`, which
  returned an empty string on a grep failure and made the gate fall through to
  `OK` with a blank count. ([#1446](https://github.com/NEWSLabNTU/nano-ros/issues/1446))
- `check-dist-runtime-deps` asks what a dist needs in order to RUN, instead of
  what any shared object in its tree happens to name. It derived the closure from
  every ELF file present, so a dist that bundles a dynamic CPython was measured
  through its `lib-dynload/` extension modules — plug-ins CPython imports
  opportunistically and degrades without. On an arm64 host with
  `[tool.arm-none-eabi-gcc]` provisioned that was twelve undeclared sonames and a
  red `just doctor`, for a toolchain that works; and declaring them was not
  available as an answer, because `libssl.so.1.1` / `libcrypto.so.1.1` are
  unobtainable on jammy and RFC-0099 D1's backward half would then have REFUSED
  the whole cross toolchain over two unimportable Python modules.

  The scope is now derived: roots are the dist's PROGRAMS (`ET_EXEC`, or `ET_DYN`
  with a `PT_INTERP`), and a library is in scope only if a root's `DT_NEEDED`
  chain reaches it inside the dist. Not the index's `front`/`smoke` entries —
  those name a release job's smoke command, not an inventory (`smoke` for
  `arm-none-eabi-gcc` names 2 of ~40 shipped binaries), and rooting the walk in a
  hand-authored field re-creates one level up the "only as complete as whoever
  wrote it" problem this gate exists to remove. The chain is walked here rather
  than left to `ldd`: measured on `[tool.xrce-agent]`, `ldd` on the program stops
  at `libmicroxrcedds_agent.so.2.4 => not found`, so `libssl.so.3` and
  `libcrypto.so.3` — real requirements three links down — are invisible to a walk
  that trusts the loader's transitivity.

  What it deliberately stops catching is written down and was measured, not
  assumed: a dlopen'd plug-in's own dependencies. The store really does contain
  required dlopen plug-ins — gcc's `liblto_plugin.so`, BFD's `libdep.so`, and the
  riscv toolchain's 75 CPython modules under `lib/python3.12/lib-dynload/`, the
  arm64 class already present on x86_64. Not one of the 88 out-of-scope objects
  names anything outside the base glibc/gcc runtime, because that toolchain
  BUNDLES what they need (`libexec/libssl.so.3`, `libsqlite3`, `libffi`, …) — so
  the loss has no content today. It is not made invisible either: an out-of-scope
  object that names an external library is REPORTED, as information rather than as
  a verdict,
  and `--include-unreached` restores the old wider measurement. Across the 8
  pinned dists provisioned on the author's host the scoped closure is identical to
  what the gate measured before, `[tool.qemu]`'s `libselinux`/`libpcre2` and
  `[tool.xrce-agent]`'s OpenSSL pair included.

  Eleven new self-test rows cover the rule in both directions — eight on
  reachability, three probing the ELF classifier against real bytes — and each is
  load-bearing. Nine source mutations are each caught by a different row: scope
  everything (the pre-fix behaviour), skip the transitive walk, key the exclusion
  on the `lib-dynload` path substring, let a library-only dist measure nothing,
  call every ELF a program, call every ELF a library, and three on the coverage
  verdict. That verdict moved into `coverage_problem()` so it is testable too,
  replacing a self-test row that read `("an undeclared-by-tool soname is a
  problem", True)`. End to end, against a synthetic store built from real host
  ELFs — neither the note path nor the failure path is reachable from the author's
  own store — the SAME soname is a note when only an unreached plug-in names it,
  a finding under `--include-unreached`, and a finding when a program names it,
  with the verdict on the first line because `just doctor` renders this gate by
  `head -1`. `file` and per-file `ldd` are gone from the walk in favour of reading
  the ELF header directly, which takes the gate from 27 s to 0.4 s. ([#1452](https://github.com/NEWSLabNTU/nano-ros/issues/1452))
- A `play_launch_parser` bump's acceptance was PROSE, and the fixture it named could pass on an artifact the previous parser baked. `nros-build` emits no `rerun-if-changed` naming the parser and the freshness probe watches repo sources, so a bumped parser moved nothing the tree examines: `nav2_compat`'s `n11_launch_xml_ros2_compat_smoke` kept asserting REAL codegen evidence from the old parser — not the `Placeholder` skip arm, a genuine pass indistinguishable from a fresh one. The fix is the missing dependency edge plus an assertion: `compile-check-fixtures.sh` stamps `tool:nros-launch-resolve=<play_launch commit>` into `.compile-ok` for every row whose staging ran `nros sync`, `compile-check-signature.sh` folds the same identity into `.inputsig`, and `require_compile_check{,_bin}` refuses a fixture baked by a different parser, naming both commits and the rebuild command (the shape `require_west_fixture` already had for `tool:nros`, #185)

  The identity is the `play_launch` commit the resolver compiled in — the same fact `nros sync` already stamps into every model it resolves (`meta.resolver`, issue 0427) rather than a second carrier, EMITTED rather than hashed so it moves iff the parser does, and CONTENT rather than a path: the SDK store accumulates (issue 0500), so an edge recorded against the store PATH names a file that still exists and never fires, which is issue 0491's trap as well

  And the prose it replaces was wrong about WHICH parser, which is why an edge on the obvious tool would have watched nothing. Measured with `strace -f -e trace=execve`: over a full `nav2_compat_smoke` build (29,828 calls) and a `cmake-configure` row (37,244), `nros-launch-resolve` runs twice each and the SDK-store `play_launch_parser` binary ZERO times. What parses is the parser crate STATICALLY LINKED into `nros-launch-resolve` from the `packages/cli/third-party/play_launch` submodule; the store binary is a standalone CLI no build path spawns. Demonstrated in the direction that used to pass wrongly: fixture baked at `9a610488`, resolver rebuilt at `eed2ade5`, test FAILS naming both — and with the resolver line stripped from the stamp (exactly what the old builder wrote) the same tree PASSES ([#1454](https://github.com/NEWSLabNTU/nano-ros/issues/1454))
- Ten more build-script sites now apply issue 1280's re-root rule, and the gate that holds them sees two populations it could not before. `nros_build_paths::env_path_list` gives a colon-separated path list the same three-valued rule a single path gets, so `nros-board-threadx` no longer compiles its kernel from one checkout and its port headers from another; the three raw `NUTTX_DIR` reads in `nros-board-common` join the sibling module that already re-rooted, so one build script stops compiling one checkout's platform port and linking another's kernel. `check-build-script-path-resolution` now also reads build-script LIBRARIES — code that runs inside every build script that depends on it — which is where seven of the ten sites were, including the zenoh-pico ABI probe's whole include set. ([#1527](https://github.com/NEWSLabNTU/nano-ros/issues/1527))
- Zephyr: zenoh-pico's tx-flush task now defaults to the read task's priority
  band instead of inheriting its creator's priority (main, k_thread 0 by default),
  so it no longer preempts the task that drains the serial RX ring. The serial
  link transmits from the UART interrupt (zenoh-pico e28ff603) rather than a
  `uart_poll_out` busy-wait in the sending thread; `CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES`
  (default 256) sizes the ring between the sender and the ISR. An image whose
  main thread registers entities above the read band still starves the reader
  while it registers: set `CONFIG_MAIN_THREAD_PRIORITY` below it (issue 1534). ([#1534](https://github.com/NEWSLabNTU/nano-ros/issues/1534))
- A Zephyr image whose components hold both a service client and a service server now compiles zenoh-pico's same-session query path (`Z_FEATURE_LOCAL_QUERYABLE`), derived from the entity inventory: before, the flag stayed at zenoh-pico's 0 on the west lane, so an in-image call went out and was never answered. `CONFIG_NROS_RMW_LOCAL_QUERYABLE` (-1 derive, 0, 1) states it; `check-knob-delivery` covers it; the boot record (version 6) carries it with the rung that decided it, and every resolved knob's rung now travels to cargo as `NROS_KNOB_SOURCE_<knob>`. ([#1549](https://github.com/NEWSLabNTU/nano-ros/issues/1549))
- A Zephyr image built with `nano_ros_add_executable` now refuses at configure when `CONFIG_NROS_DOMAIN_ID`, its entry's `system.toml` `domain_id` and any merged Kconfig fragment stating the domain (the transport snippet above all) disagree, naming the three values; before, only `nros_system_generate` compared the first two. The boot record (version 7) carries the domain the image resolved and which fragment stated it (default, kconfig, snippet, command line, or the environment on a hosted image). ([#1550](https://github.com/NEWSLabNTU/nano-ros/issues/1550))
- CI's fast lane provisions the pinned Corrosion, so a probe gate's configure resolves it from the SDK store instead of two gates racing one git clone; and wherever the store is empty, `probe-shared-types` / `probe-workspace-caps` now skip (naming `just workspace install-corrosion`) rather than clone Corrosion at all ([#1553](https://github.com/NEWSLabNTU/nano-ros/issues/1553))
- `ENTITIES` is out of the cmake grammar. It is no longer a `cmake_parse_arguments` keyword of `nano_ros_node_register` or `nros_components_register_node` — which makes the latter a keyword-for-keyword analog of upstream's `rclcpp_components_register_node` again — and the refusal a stale caller gets now reads `ARGN`, so it also catches the two spellings the parse-result test missed: `ENTITIES` swallowed into a preceding multi-value keyword, and `ENTITIES` dropped silently into `UNPARSED_ARGUMENTS`. Three `message()` remedies that had been telling readers to write the retired argument now name the two live surfaces instead (the bringup's `contract.yaml`, or a standalone leaf's `system.toml` `[[component]] entities`). `check-retired-cmake-keywords` now keeps that from recurring: a retired keyword may appear only inside its tombstone FUNCTION (it exempted all of `cmake/`, which is where the three remedies were), a tombstone's remedy may name only keywords its verb still accepts (which caught `nano_ros_entry`'s `HOST` refusal still telling readers to pass `MODEL`), and a retirement whose wording the gate cannot read fails instead of silently dropping out of the set ([#1554](https://github.com/NEWSLabNTU/nano-ros/issues/1554))
- `nros ws entity-inventory` now REFUSES an `nros-metadata.json` row carrying an
  `entities` key instead of reading it. No build has written that key since the
  `nano_ros_node_register(... ENTITIES ...)` keyword was retired; what a component
  creates is stated in the contract sidecar beside the launch file (reaching the
  verb through `--model`), or for a standalone leaf in its `system.toml`
  `[[component]]` row. A hand-edited metadata file that still carries the key gets
  an error naming where the statement belongs, rather than having it silently
  ignored. ([#1555](https://github.com/NEWSLabNTU/nano-ros/issues/1555))
- Zephyr: `CONFIG_NROS_ZENOH_LEASE_MS` now defaults to 60000, the lease every other
  platform has compiled in since issue 0906. At the old 10000 a Zephyr TCP image
  closed its session with `EXPIRED` about 30 s after opening against a stock
  `rmw_zenohd`, which keepalives an idle link every 30 s. A board that states its
  own value is unchanged. `check-zenoh-lane-ownership` now holds the Kconfig
  defaults to the cargo lane's `Z_TRANSPORT_LEASE` / `_EXPIRE_FACTOR`. ([#1574](https://github.com/NEWSLabNTU/nano-ros/issues/1574))
- A C/C++ entry configure now fails naming `nros sync` when its SystemModel does not exist, instead of silently deriving no tiers and booting single-executor (`-DNROS_ALLOW_UNSYNCED_MODEL=ON` opts out); the derived-tiers-cpp Zephyr image has a fixture lane and a cell asserting all four derived-tier nodes tick ([#1575](https://github.com/NEWSLabNTU/nano-ros/issues/1575))
- A C++ entry built freestanding (Zephyr, FreeRTOS, NuttX, ThreadX) now prints its `node "…": FAILED at …` line: `NROS_INFO`/`NROS_WARN`/`NROS_ERROR` default to the `nros_log` sink there instead of compiling to nothing (define `NROS_LOG_SINK_DISCARD` for the old no-op) ([#1576](https://github.com/NEWSLabNTU/nano-ros/issues/1576))
- `just mem-report` counts RAM sections by their writable/allocated flags, so a Zephyr image's `.noinit` RAM is included and the unattributed gap is no longer negative ([#1606](https://github.com/NEWSLabNTU/nano-ros/issues/1606))
- A publisher whose code disagrees with the QoS its contract declares is now held to it at registration: a weaker ask is raised to the declared reliability/durability, a stronger one is refused ([#1608](https://github.com/NEWSLabNTU/nano-ros/issues/1608))
- A `west-configure` fixture that failed CMake's generate step was counted as built, because its declared `output` (`CMakeCache.txt`, or the module's own `nros-system/system_config.h`) is written before generate runs — four FATAL configure errors once reported as `1 of 5 FAILED`. The three rows now declare `build.ninja`, which only a successful generate writes, so `west-fixtures.sh` keeps its one rule ("the row declares what must exist") with a premise that holds; `fixtures-manifest.py validate-compile-checks` refuses any other output for that builder. `require_west_fixture` now also requires the build's `.compile-ok` stamp, so a test no longer passes over a configure the lane counted FAILED by reading a file the failed configure left behind. ([#1627](https://github.com/NEWSLabNTU/nano-ros/issues/1627))
- A clean `nros build` of a CMake image now links the settled image on the first build: the message-bound fragment the configure reads is generated at configure time, so the derived message-bound sizes reach the first build instead of the second ([#1647](https://github.com/NEWSLabNTU/nano-ros/issues/1647))
- rv-virt-threadx (QEMU riscv64) images now print their `nros_log` records — library diagnostics, boot reports and panic text. The board registered its UART log writer only on its no-session `run_bare` entry, so every image that opened a session dropped them silently. ([#1706](https://github.com/NEWSLabNTU/nano-ros/issues/1706))
- `max-latency-runtime` now judges timer callbacks on a node that also subscribes. When no subscription had a sample, the executor fired due timers in an untimed sweep, so a timer path's overrun produced no latency verdict; the sweep now times each timer dispatch and charges it to the monitored publishers it advanced, like any other dispatch. ([#1724](https://github.com/NEWSLabNTU/nano-ros/issues/1724))
- The `rate-hierarchy-runtime` monitor no longer reports a stream at its declared rate as slightly short in the first window after start-up or arming: that window now starts at the first new sample, on the same phase as the windows after it. Silent and slow streams are still judged in their first window. ([#1725](https://github.com/NEWSLabNTU/nano-ros/issues/1725))
- The contract monitors' `/diagnostics` reporter no longer builds a 5 KB `DiagnosticArray` on the spin thread's stack for each violation: it streams the report's CDR directly (byte-identical). Its frame drops from about 10 KB to about 1 KB, which overflowed a 16 KiB Zephyr main stack on the first stored violation. ([#1726](https://github.com/NEWSLabNTU/nano-ros/issues/1726))
- The contract monitors' `/diagnostics` reporter no longer publishes inside the spin that judged the violation: verdicts are queued and sent at the end of a spin whose next timer leaves room for the report, several coalesced into one `DiagnosticArray`, and the reporter's own time is not counted as release jitter. On a slow serial link a single verdict used to cost enough spin time to trip `release-jitter-runtime` and `timer-overrun-runtime` verdicts of its own. ([#1727](https://github.com/NEWSLabNTU/nano-ros/issues/1727))
- a tiered native C++ or NuttX Rust image no longer runs its tiers' setups concurrently on the shared session (zenoh-pico's declare path is not thread-safe); both runners now serialize them through one gate, as the Linux Rust and RTOS runners already did ([#1733](https://github.com/NEWSLabNTU/nano-ros/issues/1733))
- two threads declaring or undeclaring on one zenoh session no longer corrupt zenoh-pico's declaration cache: the zpico shim serializes every call into the declare path per session (a 3% SIGSEGV in the concurrent-declare unit test is gone) ([#1749](https://github.com/NEWSLabNTU/nano-ros/issues/1749))
- `check-template-copy-out` builds each sub-project of a template from a copy, by the road its board declares. A sub-project whose SDK is absent is a named skip in the check ledger, never a pass. ([#1764](https://github.com/NEWSLabNTU/nano-ros/issues/1764))
- A `nros::main!(launch = …)` image whose model gives a node a lifecycle autostart, or names a capability axis in `execution.features`, now fails to compile with a named error when its `nros` build lacks the feature, instead of registering nothing. ([#1766](https://github.com/NEWSLabNTU/nano-ros/issues/1766))
- A host build that cargo feature unification gives the POSIX platform port (`cargo test --workspace`) no longer also links a board's own `nros_platform_*` port: `nros-platform-cffi` states the provider it compiled and every board port asks it first, so `check workspace-features` is green with the ThreadX submodule initialised. ([#1779](https://github.com/NEWSLabNTU/nano-ros/issues/1779))
- `nros build` regenerates message code whose ROS interface inputs changed since it was generated (an `apt` upgrade, a sourced overlay) instead of compiling the old code; the cmake and west roads check the same record at configure. ([#1780](https://github.com/NEWSLabNTU/nano-ros/issues/1780))
- An incremental C/C++ build after a codegen-version bump no longer compiles message libraries against a stale `include/nros/nros_config_generated.h` mirror: the mirror and its per-consumer stamp now re-run every build (write-if-changed) instead of keying on the runtime staticlib, and every consumer names both crates' mirrors. ([#1783](https://github.com/NEWSLabNTU/nano-ros/issues/1783))
- A CMake configure no longer re-runs message codegen on every pass after a package's codegen args file was rewritten once (issue 1803). The configure-time staleness check compared inputs against the generated files, which codegen leaves untouched when their bytes do not change, so they stayed "older" forever; it now compares against a stamp written each time codegen runs. ([#1803](https://github.com/NEWSLabNTU/nano-ros/issues/1803))
- quick-xml 0.37 -> 0.41 (RUSTSEC-2026-0194/0195). `package.xml` text containing an entity reference is now read whole. indicatif 0.18 drops the unmaintained `number_prefix`, and `nros`/`nros-c` use `pastey` in place of the unmaintained `paste`. ([#1805](https://github.com/NEWSLabNTU/nano-ros/issues/1805))
- A staged or installed SDK root now ships `.nros-ignore`, so one inside a workspace or checkout no longer duplicates every provider. A configure whose RMW does not resolve now prints the provider scan's own reason instead of "unknown RMW". ([#1810](https://github.com/NEWSLabNTU/nano-ros/issues/1810))
- Opening zenoh sessions from two threads at once no longer corrupts the session pool. The two opens could claim the same slot, and its config was then double-freed inside `z_open`. Opens are now serialised process-wide. ([#1835](https://github.com/NEWSLabNTU/nano-ros/issues/1835))
- A C++ entry now bakes the QoS override table for an `rclcpp`-shape component:
  the `qos_overrides.*` parameters a launch file or a contract (deadline,
  liveliness, lease) puts on the node ride on the `NodeHandle` and are installed
  before the component's constructor creates its entities. Before, only
  `configure`-shape nodes got the table and an rclcpp-shape image silently ran
  the code's QoS. A `rust` component with overrides in a C/C++ entry is now
  refused instead of silently running without them (issue 1854). ([#1854](https://github.com/NEWSLabNTU/nano-ros/issues/1854))
- `qos_overrides.<topic>.<role>.{deadline,lifespan,liveliness_lease_duration}`
  parameters are now read in nanoseconds, rclcpp's unit and the unit the launch
  resolver derives a contract's `deadline:` and `lease_duration:` into, and are
  baked in milliseconds. Before, a contract lease of 500 ms baked as 500,000,000
  ms and a deadline of 4.3 s or more was refused. A launch file that spelled
  these parameters in milliseconds must multiply by 1,000,000; a nonzero value
  under one millisecond is refused with a message saying so (issue 1855). ([#1855](https://github.com/NEWSLabNTU/nano-ros/issues/1855))
- An image that holds a parameter store but declares no `param_services` (the
  Zephyr west road states `NROS_CAPABILITY_PARAM_SERVICES=0` for it) no longer
  reserves the six services' sets and request/reply buffers in its executor
  backing. On the safety island this is 24,744 -> 1,472 B of executor storage.
  Such an image's `register_parameter_services` and `/parameter_events` calls
  are now refused by name. ([#1856](https://github.com/NEWSLabNTU/nano-ros/issues/1856))
- Two C/C++ images that differ only in their panic ending (`PANIC platform` versus none) no longer share one Corrosion cargo directory. `panic-platform` was applied after the shared cargo dir was keyed, so such images took turns rewriting one generated config header and archive. That left `check-sizes-header-mirrors` failing on the metadata probes after a full `nros sync`, including in the scheduled host-tests lane. The end-of-configure re-key now includes features applied late. Images that apply none keep their key (issue 1858). ([#1858](https://github.com/NEWSLabNTU/nano-ros/issues/1858))
- RTIC and Embassy images now deliver signaled callbacks to their nodes. `ExecutorNodeRuntime::dispatch_callback` scanned only components registered through `register_node`. The `register(runtime)` seam every `nros::main!` image uses enrolls components with the executor instead, so every callback the framework dispatch task forwarded was dropped. `just qemu test-rtic-main-e2e` builds and passes again. The router it starts listens on the port its image states in `system.toml`, and a QEMU run under `tests/run-test.sh` has a deadline, so a hang is a failure instead of a pass. ([#1865](https://github.com/NEWSLabNTU/nano-ros/issues/1865))
- `nros build` now passes `-DZEPHYR_EXTRA_MODULES=<checkout>` on its `west build` handoff. Before this fix, every Zephyr image it built against the shared store workspace (`~/.nros/workspaces/zephyr/<version>`) aborted at Kconfig with `undefined symbol NROS`, because that workspace's manifest project carries no module (issue 1258). `check-zephyr-module-binding` now also reads the CLI's Rust west handoffs (issue 1867). ([#1867](https://github.com/NEWSLabNTU/nano-ros/issues/1867))
- Zephyr serial link: a send whose frame fits the TX ring is now a copy and a
  return instead of waiting for the wire. `CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES`
  defaults to 2048 (was 256), which holds a maximum-size frame (1516 bytes)
  whole; a send that does not fit still waits, bounded, and is never dropped.
  `_z_zephyr_serial_tx_stats.frames_waited` counts the sends that did not fit.
  The serial CRC is table-driven (same value on the wire), the serializer
  COBS-encodes without a temporary frame, and the Zephyr port's framing buffers
  are static instead of two heap allocations per frame (zenoh-pico 739b1cc4,
  phase-474 I12).

### Added

- The C++ node types are MERGED: `rclcpp::Node` and `nros::Node` are one class (`std::is_same` is asserted), carrying both the freestanding out-ref `create_*` family and upstream's `shared_ptr`-returning one as overloads — a ported `create_publisher<M>("chatter", 10)` compiles hosted and FAILS TO COMPILE on a freestanding target with a diagnostic naming the out-ref overload, rather than silently gaining a heap allocation per publisher. Hosted-only state lives behind one unconditional pointer allocated lazily, so `sizeof` is 200 bytes with and without `-DNROS_CPP_STD` (it was 192 and 3752 for the two separate types), and a node driven through the out-ref family never calls `operator new`. `Node::get_logger()` now returns a logger named for the node instead of the `"nros.compat"` sentinel, `Node(name)` / `init()` / `ok()` replace upstream's throwing constructor, `nros::bind_timer` is retired in favour of `node.create_wall_timer<C, &C::method>(out, ms, self)`, the `rclcpp::TimerBase` HIERARCHY is deleted in favour of the flat `rclcpp::Timer` (the name itself survives one release as a deprecated alias whose diagnostic carries the rename), `rclcpp::create_timer(node, clock, period, cb)` arrives for clock-driven timers, and the phase-427 W8 error channel — one template `nros::ResultOf<T>` with `nros::Result` as its `void` case, both `[[nodiscard]]` — is what the merged surface returns ([#755](https://github.com/NEWSLabNTU/nano-ros/issues/755))
- C++: `nros::ComponentNode` is deleted — there is one node type, and a component
  IS one instead of wrapping one. Its members are on `nros::Node`, its 192-byte
  timer pool is the opt-in `nros::NodeWithTimers<N>` (default: no pool), and its
  macros moved to `<nros/component.hpp>`.

  Migrating a derived component:

  * `class C : public nros::ComponentNode` becomes `public nros::Node`, or
    `public nros::NodeWithTimers<N>` if it uses the storage-free timer verbs —
    `N` is the number of timers it creates.
  * the ours-only creators take `_in` names: `create_publisher_in`,
    `create_subscription_in`, `create_wall_timer_in`. The bare
    `create_publisher` / `create_subscription` / `create_wall_timer` are
    upstream's, exclusively. This rename is load-bearing rather than cosmetic:
    as overloads on one type, `create_publisher<M>("chatter", rclcpp::QoS(10))`
    bound the by-value ours-only form with no diagnostic beyond a note, so a
    ported file compiled and silently got a different type, lifetime and failure
    channel.
  * `#include <nros/component_node.hpp>` becomes `#include <nros/nros.hpp>`.
  * `NROS_SUBSCRIBE` / `NROS_CREATE_WALL_TIMER` / `NROS_COMPONENT` are unchanged
    at the call site.

  `sizeof(nros::Node)` goes 200 -> 224 bytes, hosted and freestanding alike: the
  24-byte error latch is now on every node, deliberately, because it is the error
  channel a `-fno-exceptions` target has instead of a throwing constructor.

  ([#797](https://github.com/NEWSLabNTU/nano-ros/issues/797))
- C++: the callback-group creators are renamed `_in_group` —
  `Node::create_timer_in_group`, `create_subscription_in_group`,
  `create_publisher_in_group`, and `NodeWithTimers<N>::create_timer_in_group`.
  The Rust `NodeCtx` mirror moves with them (`create_timer_in_group`,
  `create_timer_on_clock_in_group`, `create_subscription_in_group`,
  `create_publisher_in_group`).

  `_in` now means one thing: the ours-only, storage-free shape —
  `create_publisher_in`, `create_subscription_in`, `create_wall_timer_in` are
  unchanged, and so are `NROS_SUBSCRIBE` and `NROS_CREATE_WALL_TIMER`.

  Migrating: add `_group` wherever the first argument is a `nros::CallbackGroup`.

      create_timer_in<Self, &Self::tick>(grp, 10);   // before
      create_timer_in_group<Self, &Self::tick>(grp, 10);   // after

  There is no deprecated alias, so a stale call fails on the identifier. `_in` had
  meant "in a callback group" since RFC-0047 and, after the `ComponentNode` merge,
  also "ours-only" — one class, two families, one name, with
  `create_subscription_in<M, C, &C::method>(group, topic, qos)` in both. Keeping
  the old spelling as a forwarder would put both meanings back into one overload
  set, which is the thing the rename removes. The C ABI has spelled this
  `_in_group` all along (`nros_cpp_timer_create_in_group`), so the three languages
  and the ABI now agree. ([#806](https://github.com/NEWSLabNTU/nano-ros/issues/806))
- The parameter store is sized from the launch contract's `params:` (phase-446 W4): the slot count and name length follow the declared parameters, a string / array / byte-array capacity is 0 unless a declared type needs it, and the build refuses, naming the parameter and the knob, when a declared type needs a capacity that no board, Kconfig or environment value states. Zephyr gains `CONFIG_NROS_MAX_PARAM_NAME_LEN`, `CONFIG_NROS_MAX_STRING_VALUE_LEN`, `CONFIG_NROS_MAX_ARRAY_LEN` and `CONFIG_NROS_MAX_BYTE_ARRAY_LEN`, and `CONFIG_NROS_MAX_PARAMETERS` now defaults to -1 (derive). ([#872](https://github.com/NEWSLabNTU/nano-ros/issues/872))
- A node whose code declares a parameter its launch contract's `params:` do not declare, or declares one with another type, now refuses the boot naming the node, the parameter and the contract (phase-446 W6). C++ `ComponentNode::declare_parameter` checks against a generated per-node table (`nros/declared_params.hpp`); a Rust `nros::main!` image hands the same table to the executor. Nodes without `params:` are not checked, and `use_sim_time`, `start_type_description_service` and `qos_overrides.*` are always allowed. ([#878](https://github.com/NEWSLabNTU/nano-ros/issues/878))
- towncrier fragments replace a shared CHANGELOG; `just changelog-add` writes one, so two pull requests never edit the same region ([#885](https://github.com/NEWSLabNTU/nano-ros/issues/885))
- Generated C++ message headers now state the type's DERIVED serialized-size bound as `Msg::TX_MAX_SERIALIZED_SIZE` / `Msg::RX_MAX_SERIALIZED_SIZE` — the same `nros_serdes::size::max_serialized_size` number the C header and the Rust `MAX_SERIALIZED_SIZE_XCDR*` const carry — and `nros::bind_subscription` sizes its subscription's receive buffer from it instead of the older `SERIALIZED_SIZE_MAX` estimate; a type with no bound poisons the constant with a `static_assert` naming the type and the member that costs it the bound, and `nros::bind_subscription_sized` is the escape hatch ([#896](https://github.com/NEWSLabNTU/nano-ros/issues/896))
- `check-action-client-arena-budget` compares an image's `NROS_EXECUTOR_ACTION_CLIENTS` against what it actually links, so an image that budgets 0 arena slots for an action entity it does link is caught at build time instead of at runtime registration ([#900](https://github.com/NEWSLabNTU/nano-ros/issues/900))
- A parameter description has its own capacity, `NROS_MAX_PARAM_DESCRIPTION_LEN` (default 256, the most `ros2 param describe` can carry; 0 = no descriptions; Zephyr `CONFIG_NROS_MAX_PARAM_DESCRIPTION_LEN`; board `[knobs.params] max_param_description_len`). It used to share the string-value capacity, which a contract-sized store derives to 0, so a scalar-only image lost every description. A description longer than the capacity is now truncated at a character boundary and logged once per parameter, naming the knob, instead of being dropped whole in silence, and the typed builder no longer refuses a declaration over it. ([#904](https://github.com/NEWSLabNTU/nano-ros/issues/904))
- The parameter-service buffer is derived from the contract's declared parameters instead of always being the configured `NROS_PARAM_SERVICE_BUFFER_SIZE` (4,096 B a side). The entity inventory carries each node's declaration shape as `NROS_PARAM_SERVICE_SHAPE`, and nros-node bounds the worst message of all six services against the store's resolved capacities; a size any rung states still wins, and with no declaration the 4,096 default stands (Zephyr: `CONFIG_NROS_PARAM_SERVICE_BUFFER_SIZE` now defaults to the `-1` DERIVE sentinel). The reference island's 25 declared parameters size it to 2,741 B a side at the default description capacity, and 693 B stating `NROS_MAX_PARAM_DESCRIPTION_LEN=0`. ([#921](https://github.com/NEWSLabNTU/nano-ros/issues/921))
- Every RECEIVE buffer in the C++ API now sizes itself from the subscribed type's DERIVED bound where the type has one, via `nros::rx_buffer_capacity<M>` — `Subscription::try_recv` / `try_recv_validated`, `Service::try_recv_request`, `Client::call` / `call_polling`, `Future<T>` / `Stream<T>`, the action client's result and feedback, the polling tiers' result / feedback / goal, and `TickCtx::call`'s reply. Each grows a `_sized<N>` twin that takes the capacity explicitly, mirroring `bind_subscription_sized`, and a type with NO derived bound keeps the older `SERIALIZED_SIZE_MAX` estimate rather than becoming a compile error ([#964](https://github.com/NEWSLabNTU/nano-ros/issues/964))
- An image can state WHICH ENTITIES it creates -- `nano_ros_node_register(... ENTITIES ...)` -- and `NROS_EXECUTOR_MAX_CBS` derives from it instead of being hand-counted; a component that declares nothing makes the whole image refuse rather than derive a number that is short, and a publisher is counted but claims no callback slot ([#965](https://github.com/NEWSLabNTU/nano-ros/issues/965))
- Two more counts now derive from what an image declares instead of being hand-set. `NROS_MAX_LIVELINESS` (Zephyr, `ZPICO_MAX_LIVELINESS`) defaults to `-1` and sizes the zenoh liveliness pool from the entity inventory: one token for the session's own node, one per node name (plus the executor's when `lifecycle` is declared), and one per publisher, subscriber, service server and service client, three of them per action client — every token THIS session declares, never the peer graph. `NROS_RUNTIME_MAX_CELL_ENTITIES` is derived as the largest single entity kind in any one component and reaches a cargo leaf through its `nros sync` sidecar and a CMake image as `NROS_DECLARED_RUNTIME_MAX_CELL_ENTITIES`, below the board rung. Both are defaults: a stated knob, and a class's explicit `ENTITY_BOUNDS`, still win. A full component cell registry is now `NodeDeclError::CellRegistryFull` and logs the knob, where it used to be the anonymous `Runtime` error. ([#1130](https://github.com/NEWSLabNTU/nano-ros/issues/1130))
- ThreadX Rust images reserve the executor backing once: the byte pool subtracts the stated `[board.knobs.executor] backing_u64s` instead of holding the same bytes twice ([#1145](https://github.com/NEWSLabNTU/nano-ros/issues/1145))
- An RFC-0047 subnode package now builds for a freestanding target: `workspace-cpp-freertos-realtime-subnode-portable` links `subnode_pkg::SubNode` — an `rclcpp::Node` derivation with two callback groups on two tiers — for FreeRTOS on mps2-an385 (`thumbv7m-none-eabi`). The component source is unchanged; the port is a `[tiers.*.freertos]` priority pair and an `[image.freertos]` block in `deploy_bringup/system.toml`, which is the RFC-0047 portability claim measured against a cross toolchain. `subnode_freestanding_link.rs` reads the linked image and asserts the derived node is instantiated, both group-bound timer trampolines are present, and the firmware carries zero vtable/typeinfo symbols. ([#1247](https://github.com/NEWSLabNTU/nano-ros/issues/1247))
- The executor arena prices each declared subscription at its OWN type's receive bound instead of at one image-wide maximum: the message-bound inventory's join now publishes the per-type table it already computes, and `nros-node/build.rs` joins it against the declared-depth triples. A type the table does not price keeps the image-wide bound, so an image with no table is unchanged. Measured on the reference island's shape: 48,752 B of modelled arena becomes 22,832 B ([#1255](https://github.com/NEWSLabNTU/nano-ros/issues/1255))
- A zenoh service server now counts the requests its inbox drops (a full ring, or a request larger than a slot), readable with `ZenohServiceServer::inbox_drop_stats()`, and logs each episode once, naming the knob that sizes that inbox. A parameter request dropped for size now names its own length beside `NROS_PARAM_SERVICE_INBOX_BYTES`. ([#1271](https://github.com/NEWSLabNTU/nano-ros/issues/1271))
- `play_launch_parser` is now a prebuilt `dist` on linux-x86_64 and linux-arm64, not a source build: `NEWSLabNTU/nano-ros-sdk` tag `play_launch_parser-0.1.0-nros1` publishes the assets `scripts/build-play_launch_parser.sh` produces, and `nros-sdk-index.toml` names them with their sha256 and a MEASURED `floor = { glibc = "2.34" }`. The entry leaves the dist-or-reason ratchet and gains a measured `smoke` probe (`bin/play_launch_parser --help`), so it leaves the smoke-or-reason ratchet too. `system` gains `libpython310`: the dist links `libpython3.10.so.1.0` as a hard single-minor `DT_NEEDED` (pyo3 `auto-initialize`), which is what D1's backward half reads to refuse it on a host shipping 3.12 rather than download a binary that cannot start. macos-arm64 was BUILT and PUBLISHED and is deliberately NOT declared — its `LC_LOAD_DYLIB` names an absolute `/Library/Frameworks/Python.framework/Versions/3.14/Python` with no `@rpath`, so it cannot start on a mac lacking that exact framework, and the backward half that would catch it is Linux-only ([#1273](https://github.com/NEWSLabNTU/nano-ros/issues/1273))
- `nros::Time::to_ros_msg()` converts a `Time` into any generated message shaped `{int32 sec, uint32 nanosec}` — the Rust counterpart of the C++ `Time::to_msg` template. Codegen emits the `SecNanosecMsg` impl by field shape, so `builtin_interfaces/msg/Duration` binds too; the conversion is infallible and returns the message, not a `Result`. ([#1318](https://github.com/NEWSLabNTU/nano-ros/issues/1318))
- The zenoh backend now SERVES `TRANSIENT_LOCAL` durability on a publisher, so a zenoh action server starts again and its `/status` topic keeps the contract `rcl_action_qos_profile_status_default` states. The publisher retains its last sample and declares a queryable on `<topic keyexpr>/@adv/pub/<zid>/<eid>/_`, which is where a stock `rmw_zenoh_cpp` subscriber's history query lands — measured off a live `rmw_zenohd` rather than recalled. The graph token advertises the durability that is served (`ros2 topic info -v` reads `Durability: TRANSIENT_LOCAL`, `History (Depth): KEEP_LAST (1)`), and a stock transient-local reader attaching after a goal terminated receives the retained sample where a volatile one receives nothing. A SUBSCRIPTION still refuses `TRANSIENT_LOCAL` — querying a peer's cache on match is a separate capability — and the retention pool is sized by `ZPICO_MAX_TL_PUBLISHERS`, which an image declaring its endpoints derives and which refuses at BUILD time a stated value below the declared demand.

  What this does NOT fix, and it was measured on the same run: a nano-ros action server never publishes a terminal status sample at all, on any backend, because `complete_goal_raw` removes the goal from `active_goals` before publishing. The retained sample is therefore the empty array the server really did send last. Issue 1361. ([#1341](https://github.com/NEWSLabNTU/nano-ros/issues/1341))
- `just check zephyr-workspace-foreign-checkout` refuses a Zephyr west workspace that names ANOTHER nano-ros checkout. A workspace inside one checkout whose `nano-ros` manifest project pointed at a different one made `west build` compile the application from one tree and the nano-ros Zephyr module — the zpico C shim, the platform and board headers, the RMW dispatch, the knob inventory — from another, silently, for a month; a mixed image builds, links and runs, and the two halves disagree only when one of them changes. The gate applies the same rule the build already uses for inherited paths (a path outside any checkout is fine, a path into a DIFFERENT checkout is not) to three places workspace state records it: the manifest project, every `CMakeCache.txt` in the workspace's build dirs, and the venv console scripts' shebangs — which carry the crossing into `WEST_PYTHON` and survive repairing the link. It is buildless and on the push lane, records a skip rather than an OK when no workspace is provisioned, and `just doctor` reports the same answer. `just zephyr setup` now also unbinds an already-provisioned workspace, so re-running it repairs the state instead of leaving it. ([#1387](https://github.com/NEWSLabNTU/nano-ros/issues/1387))
- The sizing descriptor's `[params]` section is now FILLED and READ, so an image's
  parameter store is sized from the contract on the descriptor road as well as the
  CMake one.

  The schema gained the section; this wave is the vertical slice through it. The
  shared composer `sizing_descriptor::build` derives `[params]` from
  `ParamDeclarations` — so BOTH producers get it from ONE derivation, which is what
  keeps a workspace image and a cargo leaf from disagreeing — and `nros-params` /
  `nros-node` read it through `nros_sizing_descriptor::from_build_env()`, at the
  rung the `NROS_DECLARED_*` carrier already occupied.

  The three states map to the three answers the schema can carry. `Absent` writes
  nothing: an image whose contract says nothing about parameters has not been asked,
  so a refusal there would put a reason in front of every reader of every descriptor
  to say that a feature nobody used was not used. `Refused` carries the derivation's
  OWN prose, which already names the silent nodes, with the four consequent fields
  saying they are consequences. `Declared` states all seven.

  `[params]` is deliberately NOT folded into `[meta] status`. That summary's members
  are facts every image has; a parameter-store gap is not, and folding it in would
  be wrong in both directions — `Absent` would make every descriptor in the tree read
  `partial`, and `Refused` would let one image's half-authored `params:` cost every
  unrelated derivation in the file its status.

  Measured, four builds differing only in `NROS_SIZING_DESCRIPTOR`: no descriptor,
  an empty `[params]` and a fully refused `[params]` all emit a BYTE-IDENTICAL
  `nros_params_config.rs` (`MAX_PARAMETERS = 32`, `MAX_PARAM_NAME_LEN = 64`,
  `DECLARED_PARAM_SERVICE_SHAPES = None`), and a stated one emits 5 / 15 /
  `Some(&[[3,24,0,0,1,1,0,0,0],[2,27,0,0,0,0,0,0,0]])` — byte-identical to the same
  contract delivered by the nine env carriers, which is the parity proof.

  The carriers are NOT retired. Three roads still have no descriptor at all — a
  standalone cargo leaf with no model, a multi-entry cmake configure, and the Zephyr
  west lane — so they stay KEPT in `check-knob-single-reader.py`, with their ledger
  reasons rewritten: "the schema has no parameter section" is no longer true, and a
  reason nobody re-examines is the failure that ledger exists to prevent. ([#1408](https://github.com/NEWSLabNTU/nano-ros/issues/1408))
- The boot report carries the platform heap's high-water mark and its capacity (`heap_peak_bytes`, `heap_capacity_bytes`), written by `nros-platform-zephyr` from `nros_platform_alloc` and `nros_platform_realloc` -- the only two places the peak can move -- so a console-less board answers "how much did it need" after a halt; `just check heap-headroom` (`read-boot-report.py --heap-headroom <dump>`) refuses a dump whose `capacity - peak` is below 24576 and refuses a peak of 0, because an unmeasured image and a measured-and-fine image must not read the same ([#1424](https://github.com/NEWSLabNTU/nano-ros/issues/1424))
- Zephyr: `CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES` sizes the ring between the
  UART interrupt and zenoh-pico's read task on the serial link (default 1024, as
  before). At 921,600 baud 1 KiB is 11 ms of line; raise it when the read task
  can be kept off the CPU longer than that, for example while the image's own
  declarations go out at registration (issue 1534). ([#1534](https://github.com/NEWSLabNTU/nano-ros/issues/1534))
- `python3 scripts/lib/repo_walk.py PATTERN…` lists untracked files in THIS checkout only — it stops at every nested repository (an agent worktree under `.claude/worktrees/`, a submodule, a nested clone), which a `find .` walks straight into; `check-repo-root-walk-scope` holds committed scripts to the same scope ([#1565](https://github.com/NEWSLabNTU/nano-ros/issues/1565))
- A managed node now announces its transitions on `~/transition_event`, so a supervising node can watch a lifecycle node passively instead of polling `~/get_state`. `register_lifecycle_services` creates the `lifecycle_msgs/msg/TransitionEvent` publisher REP-2002 requires beside the five services, `LifecyclePollingNodeCtx::trigger_transition` records every transition it performs — the one function `ros2 lifecycle set`, the C and C++ `change_state` shims, `autostart` and the safe `LifecycleCallbacks` road all funnel through — and the executor drains the record onto the wire during `spin_once`. Failed transitions are announced too, because the failure is what a supervisor needs to see; rejected ones are not, because none ran. The QoS is upstream's, MEASURED rather than assumed: `ros2 topic info -v` on a live humble `lifecycle_talker` reports RELIABLE / VOLATILE, i.e. `rmw_qos_profile_default`, so the publisher costs one publisher slot and one liveliness token per lifecycle-enabled image and **no queryable** — phase-467's study had priced this row as its most expensive on the premise that the topic is TRANSIENT_LOCAL, which would have put a sixth slot into a `ZPICO_MAX_QUERYABLES` that defaults to 8 against the eleven `[param_services]` and `[lifecycle]` already claim. `InfraServices::publishers` carries the new slot into `EntityInventory::derive`, and `check-infra-queryable-counts` holds the count to its creation site. This closes the last `gap` row in the lifecycle ledger shard. ([#1587](https://github.com/NEWSLabNTU/nano-ros/issues/1587))
- nano-ros nodes can publish `/rosout`, so `ros2 topic echo /rosout`, `rqt_console` and launch-side log aggregators see them: turn on the `rosout` feature, create an `rcl_interfaces/msg/Log` publisher with `nros::rosout::qos()` (upstream's profile) or `nros::rosout::qos_bounded()` (VOLATILE, and free of the zenoh queryable slot a transient-local publisher costs), call `nros::rosout::enable()` and `nros::rosout::pump()` from your spin loop. The sink deliberately does NOT publish — it copies into a bounded static ring, because a sink that enters the transport on a path that can itself log is unbounded recursion — and the ring costs 5 760 B of `.bss` on the defaults, nothing at all when the feature is off. C and C++ reach none of it yet (issue 1589) ([#1589](https://github.com/NEWSLabNTU/nano-ros/issues/1589))
- On the cargo and cmake roads, an image whose bringup declares `param_services`, or whose launch file seeds a `<param>`, now carves its parameter store from the executor backing. It no longer asks the RTOS heap for one 280 KB allocation, so a FreeRTOS Cyclone C++ image no longer refuses the store at the 640 KiB heap default. The sizing descriptor gained `[params] store` (`declared`, `param_services`, `launch_seed` or `none`). A bringup with no contract still gets a descriptor carrying only that field when it implies a store. An image without parameters builds byte-identically. On a single-package Rust leaf, the axis counts only together with the `param-services` feature on its own `nros` dependency. ([#1706](https://github.com/NEWSLabNTU/nano-ros/issues/1706))
- The contract-violation ring keeps the LATEST `CONFIG_NROS_EXECUTOR_MAX_VIOLATIONS` (default 8) verdicts instead of the first, numbers them and counts what it evicted. A board with no wired console can read every stored violation over SWD from the `NROS_VIOLATION_RECORD` symbol (with `CONFIG_NROS_BOOT_REPORT`; decode with `scripts/read-violation-record.py`), see it in a trace as markers 21-24 (with `CONFIG_NROS_TRACE_CALLBACKS`), or log each one with its number and the counters at the end of its spin (`CONFIG_NROS_VIOLATION_DRAIN_REPORT`). ([#1714](https://github.com/NEWSLabNTU/nano-ros/issues/1714))
- Contract monitors can wait for the application to finish start-up: with `CONFIG_NROS_MONITOR_ARM_ON_CALL` they arm when the application calls `nros_monitors_arm()` / `nros::arm_monitors()` / `nros::monitor::request_monitor_arming()` (for example on entering RUN), with `CONFIG_NROS_MONITOR_ARM_GRACE_MS` as a deadline after the first spin. Verdicts before arming are counted (`suppressed_before_arm`) but not stored. The default is unchanged: armed at the first spin. ([#1715](https://github.com/NEWSLabNTU/nano-ros/issues/1715))
- The Zephyr boot record (layout 11) keeps the platform heap's peak at the first spin beside the running peak, and `read-boot-report.py --heap-headroom` says which one it judged and names `CONFIG_NROS_ZEPHYR_HEAP_SIZE` without the 512 B slab the capacity word includes. A stated value of a derivable pool knob below what the image's own inventory derives is now a configure warning. ([#1728](https://github.com/NEWSLabNTU/nano-ros/issues/1728))
- The callback trace can record each sample a subscription takes (marker 25, with the sample's source stamp as 26/27 when its place is known) and can keep one timer tick in N: per callback with `nros_trace_set_take` / `nros_trace_set_timer_every`, or image-wide with `CONFIG_NROS_TRACE_TAKES` and `CONFIG_NROS_TRACE_TIMER_EVERY`. ([#1730](https://github.com/NEWSLabNTU/nano-ros/issues/1730))
- `param-services` no longer needs `alloc`: an image whose build knows it carves a parameter store (`NROS_PARAM_STORE=1`, or parameters declared in the contract) now carves the service state, the shared request/reply buffers and a staging slot for incoming values from the executor backing, so `ros2 param` works on an image with no heap. A remote `set_parameters` no longer holds a ~8.5 KiB parameter value on the stack (measured 26 KiB -> 352 B per request in release). A heap-free build with no carved store is refused at compile time, naming `NROS_PARAM_STORE`. ([#1812](https://github.com/NEWSLabNTU/nano-ros/issues/1812))
- A C++ node's callback groups are now held to its registration's `CALLBACK_GROUPS` at boot. Two cases are refused: `create_callback_group` with a name the keyword does not declare, and a declared group that the constructor never creates when more than one is declared. Each refusal halts boot with code -459, naming the group and the declared list. One declared group and none created is node-level placement, and is still allowed. The check runs in both the generated workspace entry and the `NROS_COMPONENT` factory (phase-459 W6).
- An image can state a build knob as `[image.<id>.knobs.<group>] <name> = <value>` in `system.toml`, in the same shape a board descriptor's `[knobs.<group>]` uses (`[image.zephyr_xrce.knobs.zephyr] heap_size = 98304`). The key must be a knob in the registry, and the value is checked against the knob's type and range. A typo stops `nros sync` / `nros build` with an error that names the key. The value reaches the build exactly as the `NROS_*` variable would.

  An `[image.<id>] env` row whose name is a registry knob (for example `NROS_ZEPHYR_HEAP_SIZE`) still works in this release and prints a warning naming its `knobs` spelling. It will be refused after the next release. Stating one knob both ways in an image is an error. `env` rows for variables that are not registry knobs are unchanged.
- An unmodified ROS 2 C++ program, `main` included, now runs on FreeRTOS and
  Zephyr. Build it with `nano_ros_add_executable(<name> <sources> ROS2_MAIN)`:
  the program's `main` is reached through the board's startup, and the ported
  sources get the C++ standard library they were written against.
  `examples/templates/cpp-port-minimal-publisher/` shows the build glue for
  FreeRTOS (`mps2-an385-freertos/`) and Zephyr (`zephyr/`) beside the shared,
  unmodified source. Zephyr `native_sim` is not supported for a ported program,
  because its C library cannot host libstdc++; use a Cortex-M board such as
  `mps2/an385`.

  Fixed: on Zephyr, a source file that called `rclcpp::init` and included
  `<rclcpp/rclcpp.hpp>` but not `<nros/main.hpp>` ignored
  `CONFIG_NROS_ZENOH_LOCATOR` and connected to the guest's own loopback.
  `nano_ros_add_executable` also refused a Zephyr leaf whose `system.toml` says
  `[image.zephyr] board = "zephyr"`.
- C API: `<rclc/rclc.h>` (and every other upstream-path header) now also carries
  `RCLC_C_COMPONENT`, so C code needs no `<nros/*.h>` include at all. The
  examples, the book and `nros new` include only `<rcl/*.h>` / `<rclc/*.h>`;
  `<nros/*.h>` is internal and will be renamed.
- C API: the public headers (`<rcl/*.h>`, `<rclc/*.h>`, `<rcl_action/*.h>`,
  `<rcl_lifecycle/*.h>`, `<rclc_lifecycle/*.h>`, `<rclc_parameter/*.h>`,
  `<rosidl_runtime_c/*.h>`) are freestanding: they compile with
  `-ffreestanding -nostdinc`, so a target with no libc can include them. They no
  longer pull in `<stdio.h>` or `<string.h>`; a C file that called `printf` or
  `memcpy` without including those headers itself must now include them.
- The `rclcpp-compat-smoke` and `topic-state-monitor-port` templates now run
  unmodified on FreeRTOS (mps2-an385) and Zephyr (mps2/an385) as well as posix,
  each with a `mps2-an385-freertos/` and a `zephyr/` build directory beside its
  source. A ported node whose `main` loops on `rclcpp::spin_some` and
  `std::this_thread::sleep_for` works on FreeRTOS: the board now provides the
  `sleep`/`usleep` and `gettimeofday` calls the C++ library makes, so
  `std::chrono::steady_clock` counts from boot instead of returning garbage. On
  Zephyr, `ament_target_dependencies(<name> …)` after
  `nano_ros_add_executable(<name> …)` now reaches the `app` target that holds the
  sources, so a dependency such as `diagnostic_updater` is on their include path.
- `--ros-args -p [node:]name:=value` and `--params-file <file>` are honoured, by
  `nros::init_with_args` / `nros::Context::new` in Rust and by
  `rclcpp::init(argc, argv)` in C++, in any image with a parameter store
  (`param-services` / `param-store`).

  The value replaces the default when a node declares the parameter, on every
  road: C, C++, Rust and the launch file's baked values. The last matching
  override wins; `node:` reaches only that node; the text is typed by the
  declared default.

  A parameter file uses the ROS 2 layout (`<node>: ros__parameters: …`, `/**` for
  every node, nested keys become dotted names) and holds scalars. Whatever else a
  file contains is refused, naming the line. A value that does not parse as the
  declared type is logged at error level and the default stays. Images without a
  parameter store keep refusing both flags by name.
- `NROS_XRCE_MAX_SUBSCRIBERS`, `NROS_XRCE_MAX_SERVICE_SERVERS`, `NROS_XRCE_MAX_SERVICE_CLIENTS`, `NROS_XRCE_BUFFER_SIZE` and `NROS_ZENOH_SOCKET_TIMEOUT_MS` are knob-registry rows: a project states them as `[image.<id>.knobs.xrce] max_subscribers` / `max_service_servers` / `max_service_clients` / `buffer_size` and `[image.<id>.knobs.zenoh] socket_timeout_ms`. An `[image.<id>] env` row naming one still works and warns as deprecated, and on Zephyr their Kconfig symbols lose their prompt for a project with a `system.toml` (set them there, or with `-DNROS_OVERRIDE_<ID>`, not in `prj.conf`). Defaults and the derived XRCE counts are unchanged.
- `NROS_XRCE_SUBSCRIBER_RING_DEPTH`, `NROS_XRCE_SUBSCRIBER_BUFFER_SIZE`, `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE`, `NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE` and `NROS_XRCE_TRANSPORT_MTU` are knob-registry rows: a project states them as `[image.<id>.knobs.xrce] subscriber_ring_depth` / `subscriber_buffer_size` / `service_request_buffer_size` / `service_reply_buffer_size` / `transport_mtu`. An `[image.<id>] env` row naming one still works and warns as deprecated, and on Zephyr their Kconfig symbols lose their prompt for a project with a `system.toml` (set them there, or with `-DNROS_OVERRIDE_<ID>`, not in `prj.conf`). Defaults, ranges and the derived subscriber entry size are unchanged.
- `rclcpp_lifecycle::LifecycleNode` is available in C++, under upstream's name
  and include path (`<rclcpp_lifecycle/lifecycle_node.hpp>`). It IS an
  `rclcpp::Node`:

  - upstream's constructor;
  - the `on_configure(const rclcpp_lifecycle::State&)` … `on_shutdown`
    overrides returning `CallbackReturn::SUCCESS` / `FAILURE` / `ERROR`;
  - `configure()`, `activate()`, `deactivate()`, `cleanup()`, `shutdown()` and
    `trigger_transition()`, each returning the resulting state;
  - `get_current_state()`;
  - `create_publisher<M>(topic, qos)`, returning a managed `LifecyclePublisher`
    that sends only while the node is Active.

  So a ported lifecycle node's class body compiles unchanged. The REP-2002
  services are registered by the constructor.

  `nros::LifecycleNode`, the mixin a component bound to its node with
  `bind(Node&)`, is deprecated and will be removed in the next release. Derive
  from `rclcpp_lifecycle::LifecycleNode` instead and drop the `bind` /
  `register_services` / `autostart` calls; a component that self-drives calls
  `configure()` and `activate()`.

### Performance

- Parameter services no longer cost 48 KiB of buffers per node: every node's six servers share one request/reply buffer pair per executor (8 KiB at the default `NROS_PARAM_SERVICE_BUFFER_SIZE`). The derived queryable pool (`NROS_MAX_QUERYABLES`, `NROS_XRCE_MAX_SERVICE_SERVERS`) now counts six servers per node when the bringup declares `param_services`, and five when it declares `lifecycle`. ([#1270](https://github.com/NEWSLabNTU/nano-ros/issues/1270))

### Breaking

- C++: the node type is `rclcpp::Node`, and `nros::Node` is a **deprecated alias**
  for it. They are one class — `std::is_same<rclcpp::Node, nros::Node>::value` is
  true — so migrating is textual:

      nros::Node node;                       // before, now warns
      rclcpp::Node node;                     // after

      ::nros::Result configure(::nros::Node& node);      // before
      ::nros::Result configure(::rclcpp::Node& node);    // after

  This is the tenth and last type of the `nros::` → `rclcpp::` move (RFC-0089:
  `rclcpp::` is the home of our C++ API). The other nine — `Clock`, `Duration`,
  `Time`, `Publisher<M>`, `Subscription<M>`, `Client<S>`, `Service<S>` and the two
  action types — moved earlier and their `nros::` aliases are NOT deprecated yet.

  The alias stays for at least one release: `nros-v0.5.0` shipped the old spelling
  in the `examples/templates/**` a user copies out, so an existing project gets a
  compiler warning naming its replacement rather than an unknown identifier. Build
  with `-Werror=deprecated-declarations` to find your call sites.

  `NROS_CODEGEN_VERSION` moves 3 → 4 because one signature generated code names
  changed spelling. The accepted floor is unchanged, so an already-generated tree
  still compiles; `nros sync` regenerates it if you would rather not see the
  warning. ([#929](https://github.com/NEWSLabNTU/nano-ros/issues/929))
- `nros-log`'s configuration is now the `[knobs.log]` tenant, and its pick-one
  Cargo features are **deprecated** — they still build for one release:

  | deprecated feature    | knob                       | builtin |
  | --------------------- | -------------------------- | ------- |
  | `max-level-*`         | `NROS_LOG_MAX_LEVEL`       | `trace` |
  | `buffer-size-<N>`     | `NROS_LOG_BUFFER_SIZE`     | 256     |
  | `early-records-<N>`   | `NROS_LOG_EARLY_RECORDS`   | 4       |
  | `rosout-records-<N>`  | `NROS_LOG_ROSOUT_RECORDS`  | 16      |
  | `dynamic-loggers-<N>` | `NROS_LOG_DYNAMIC_LOGGERS` | 16      |

  State a knob the way every other one is stated — `[image.<id>] env = {
  NROS_LOG_MAX_LEVEL = "info" }` in `system.toml`, `[board.knobs.log] max_level =
  "info"` in a board descriptor, `CONFIG_NROS_LOG_MAX_LEVEL` on Zephyr, or the
  environment — and `nros config explain` prints which rung decided it.

  While the features last, a feature with no stated knob is honoured with a
  `cargo:warning`, a feature that agrees with a stated knob is reported as
  redundant, and a feature that **disagrees** with a stated knob, or with another
  member of its family, **fails the build** rather than picking one silently.

  Two behaviour changes to check for:

  - `max-level-trace` and `buffer-size-256` are no longer in `nros-log`'s
    `default` features (a default member would have disagreed with every stated
    knob). Their values are the builtins, so a build that named nothing is
    unchanged. A build that named `features = ["max-level-info"]` **with**
    default features used to get `trace` — the old `const fn` tested
    `max-level-trace` first — and now gets `info`, which is what it asked for.
  - The ceiling now also bounds C and C++ records: `Logger::is_enabled` (and so
    `nros_logger_is_enabled` and every `NROS_LOG_*` macro) refuses a severity
    below `NROS_LOG_MAX_LEVEL`. Before, the `max-level-*` features filtered only
    Rust call sites.

  The platform clock behind `Record::timestamp_ns` and the `nros_*_throttle!`
  macros is now also enabled by the platform's `[capabilities] clock = true`
  (declared by every in-tree port), so a pure-Rust image built for a platform
  gets timestamps without naming `nros-log/platform-clock`. The feature remains
  and is not deprecated: it only pulls code in. Issue 1037.

  ([#1037](https://github.com/NEWSLabNTU/nano-ros/issues/1037))
- The std-flavoured C++ surface — `rclcpp::Node`, the `std::shared_ptr`-returning
  `create_*` factories, the `std::string` / `std::function` / `std::chrono`
  overloads in `nros/std_compat.hpp` and the `RCLCPP_*_STREAM` macros — is now
  reachable **only** by defining `NROS_CPP_STD`. Until now the six
  `NROS_CPP_HAS_*` capability macros were also discovered from the include path
  with `__has_include`, so any hosted compiler handed you that surface whether you
  asked for it or not.

  **This breaks out-of-tree consumers that relied on the discovery.** If your
  build compiled ported rclcpp code without defining `NROS_CPP_STD`, it will now
  get the freestanding API where it used to get the std one, and the failure is a
  compile error naming an unknown `rclcpp::Node` or a `create_publisher` overload
  that does not match. The fix is one line —
  `target_compile_definitions(<target> PRIVATE NROS_CPP_STD=1)` — and if you reach
  nano-ros through `cmake/compat/NrosRclcppCompat.cmake` you need nothing at all,
  because that path now sets the definition itself (it previously set none, and
  worked only because a hosted compiler happened to find libstdc++).

  There is **no deprecation cycle, and there cannot be one**: the change is a
  macro that stops being defined, and nothing can warn on that. This note is the
  whole migration path.

  The reason for the removal is that discovery was not merely over-broad, it was
  wrong on the lane it mattered on. `__has_include(<string>)` answers TRUE on the
  pinned arm-none-eabi 13.2 under `-ffreestanding`, where libstdc++ 13's
  `bits/requires_hosted.h` makes the include a hard `#error` — so **no embedded
  C++ FreeRTOS image compiled at all** (issue 1187). Per-header parsing on that
  toolchain moves from 26 pass / 20 fail to 47 pass / 0 fail, and
  `scripts/build/fixtures-build.sh freertos cpp zenoh` now produces six ARM ELF
  binaries where none built before. `__STDC_HOSTED__` is not a repair: it is 0 on
  that lane but 1 on Zephyr's `-nostdinc++` minimal libcpp, where `<string>` is
  absent. Each probe is right on exactly one embedded lane; only the request is
  right on both, because it is not a probe.

  Which surface a consumer is on is documented in the book
  (`C++ API — two surfaces`, and `Porting a ROS 2 C++ node`) and in
  `docs/reference/c-api-cmake.md`. The short version: the split is ported rclcpp
  code versus code written for nano-ros, and it is **not** host versus embedded —
  a native host build of a nano-ros-native node does not want this flag. ([#1187](https://github.com/NEWSLabNTU/nano-ros/issues/1187))
- `system.toml` no longer accepts `[deploy.*]` or `[param_services]`; both are refused by name. Move what a block BUILT (`board`, `target`, `rmw`, `profile`, `features`) to `[image.<id>]`, a board's site facts (`[deploy.<n>.nros]`) to `[board_config.<board>]`, and WHERE nodes run (`nodes`, `launch`, `domain_id`, `locator`) to `[host.<name>]`. Replace `[param_services]` with `[system] features = ["param_services"]`. Both were deprecated since phase-383 and warned on every build; the version is now 0.6.0. ([#1809](https://github.com/NEWSLabNTU/nano-ros/issues/1809))
- - `nros::StandaloneNode`, `nros::NodeConfig`, `nros::PublisherHandle` and
    `nros::SubscriptionHandle` are no longer re-exported. `nros::Node` is the
    node. Code that wants the transport-less standalone node imports it from
    `nros_node`.
  - A ported rclrs program can keep its own `use rclrs::*;`. Rename the
    dependency in `Cargo.toml` (`rclrs = { package = "nros", ... }`), or write
    `use nros as rclrs;`.
- A Rust component's `register` now gets the same `nros::Node` a standalone
  program gets from `executor.create_node(name)`. Before, `ctx.create_node(...)`
  returned a `DeclaredNode`, a different type, so node code could not be shared
  between a component and a program.

  - The constructors that declare an entity by stable id and bind its callback by
    name (`create_publisher_for_topic`, `create_timer_for_callback_name`,
    `callback_for_name`, …) are now methods of the `nros::DeclarativeNode` trait.
    Add `use nros::DeclarativeNode;` (or use `nros::prelude::*`) where a component
    calls them.
  - Four explicit-id forms would otherwise have shadowed rclrs's names on `Node`,
    so they are now `declare_*`: `declare_publisher`, `declare_subscription`,
    `declare_timer` and `declare_action_server` (plus their `_with_qos`,
    `_on_clock` and client siblings).
  - The component API now needs the `rmw-cffi` feature, because registration
    creates real nodes.
  - `nros::record_node_metadata` takes the executor as well as the recorder.
  - `DeclaredNode`, `NodeRuntimeAdapter`, `RuntimeNodeRecord`,
    `DeclaredNodeRuntime`, `NodeExecutorRuntime` and `MISSING_NODE_EXPORT_ERROR`
    are gone.
- C API: `nros_timer_t` no longer has a `last_call_time_ns` field. Nothing
  updated it after a timer was added to an executor, so it was never the time
  of the last call; read `nros_timer_get_time_since_last_call` or
  `rcl_timer_get_next_call_time`, which ask the executor.
- C API: a service server or client the executor dispatches no longer carries
  its own entity storage or callback — the executor arena owns them.
  `nros_service_t` is 736 bytes instead of 2368 on x86_64, `nros_client_t` 744
  instead of 2376. What a C caller changes:

  - `nros_service_init_with_qos(service, node, type, name, qos)` and
    `nros_service_init_with_options(..., qos, options)` no longer take
    `callback` and `context`.
  - `nros_executor_add_service(exec, service)` is gone; use
    `nros_executor_add_service_raw(exec, service, callback, context)`.
  - Generated typed services: `<Srv>_service_init(&service, &node, "/name")`
    no longer takes the handler; register with
    `<Srv>_executor_add_service(&exec, &service, &handler)`. Regenerate
    (`nros sync`): the codegen version is now 12 and older trees are refused.
  - `nros_client_set_response_callback` must be called after
    `nros_executor_add_client`; it returns `NROS_RET_NOT_INIT` before.
  - Polled servers and clients are the new types `nros_polling_service_t` and
    `nros_polling_client_t`, with `nros_polling_service_*` /
    `nros_polling_client_*` replacing `nros_service_init_polling`,
    `nros_service_take_request_raw`, `nros_service_send_response_raw`,
    `nros_client_init_polling`, `nros_client_send_request_raw`,
    `nros_client_take_response_raw` and the two `set_wake_callback`s.
- C API: a subscription the executor dispatches no longer carries its own
  subscriber storage, callback or context — the executor arena owns them.
  `nros_subscription_t` is 744 bytes instead of 2536 on x86_64 (700 instead of
  2368 on Cortex-M3). What a C caller changes:

  - `nros_subscription_init_with_qos(sub, node, type, topic, qos)` and
    `nros_subscription_init_with_options(sub, node, type, topic, qos, options)`
    no longer take `callback` and `context`. Pass them at registration instead.
  - `nros_executor_add_subscription(exec, sub, invocation)` is gone. Use
    `nros_executor_add_subscription_raw(exec, sub, callback, context, invocation)`
    (CDR bytes) or `nros_executor_add_subscription_typed` (a deserialised
    message, which the generated `<Msg>_executor_add_subscription` macro wraps).
  - `nros_executor_add_subscription_in_group` takes `callback` and `context`
    after the subscription.
  - A subscription you drain yourself is the new type `nros_polling_subscription_t`,
    and its functions are `nros_polling_subscription_*`:
    `nros_subscription_init_polling[_with_qos]` became
    `nros_polling_subscription_init[_with_qos]`, and `take_serialized`,
    `take_sequence`, `take_validated`, `borrow`, `release`, `set_wake_callback`,
    `get_actual_qos` and `fini` moved the same way.
- C API: an action server or client the executor dispatches no longer carries
  the polled form's storage. `nros_action_server_t` is 824 bytes instead of
  7592 on x86_64, `nros_action_client_t` 736 instead of 6296. Callback-mode
  code is unchanged. Polled action code moves to the new types
  `nros_polling_action_server_t` and `nros_polling_action_client_t`, and its
  functions are renamed `nros_polling_action_server_*` /
  `nros_polling_action_client_*`: `nros_action_{server,client}_init_polling`
  became `nros_polling_action_{server,client}_init`, and every `_raw`,
  `set_*_wake_callback` and `fini` call moves the same way. The polled server
  answers `nros_polling_action_server_get_active_goal_count`,
  `_goal_exists` and `_expire_goals`.
- C API: generated messages take rosidl's C names and member shape, and the
  type support is rosidl's macro (RFC-0089, phase-492 W7/W8). Codegen version 14;
  regenerate C trees (`nros sync`). What a C caller changes:

  - Names: `std_msgs_msg_string` is `std_msgs__msg__String`, included as
    `<std_msgs/msg/string.h>` (rosidl's snake-case file name). Services are
    `<pkg>__srv__<Name>_Request` / `_Response`, actions
    `<pkg>__action__<Name>_Goal` / `_Result` / `_Feedback`. Functions take `__`:
    `<Msg>__serialize`, `__deserialize`, `__deserialize_view`, `__View`;
    constants `<Msg>__<CONST>`, bounds `<Msg>__TX_MAX_SERIALIZED_SIZE`.
  - Member shape: a string is `rosidl_runtime_c__String {data, size, capacity}`,
    a sequence `rosidl_runtime_c__<elem>__Sequence` / `<Msg>__Sequence`, over
    memory the CALLER owns. `msg.data` is `msg.data.data`; set `.size`. Wire a
    message with the generated `<Msg>__init_in(&msg, &storage)` over a
    `<Msg>__Storage`, or point `data` at your own buffer (micro-ROS's idiom).
    A message with no strings or sequences needs no wiring (`= {0}`).
  - `<Msg>_init` / `_fini` are gone, and rosidl's allocating `__init` / `__fini`
    / `__create` / `__destroy` are not declared (a call fails to compile naming
    `__init_in`). Deserialising writes into `capacity` and fails when a field
    does not fit; it never allocates. RFC-0033 `heap` mode no longer applies to
    C (a `heap` entry only sizes `__Storage`); Rust and C++ are unchanged.
  - Type support: `<Msg>_get_type_support()` is
    `ROSIDL_GET_MSG_TYPE_SUPPORT(pkg, msg, Name)` (and `_SRV_` / `_ACTION_`),
    from `<rosidl_runtime_c/message_type_support_struct.h>`. It yields nano-ros's
    descriptor, which now carries `serialize` / `deserialize` (services and
    actions per part). `rosidl_message_type_support_t` stays undeclared.
  - Publish with `rcl_publish(&pub, &msg, NULL)`; the generated `<Msg>_publish`
    is gone. Register a typed subscription with rclc's six arguments,
    `rclc_executor_add_subscription_with_context(&exec, &sub, &msg, cb, ctx,
    invocation)`; `rclc_executor_add_subscription_typed{,_sized}`,
    `<Msg>_executor_add_subscription*` and `<Msg>_subscribe*` are gone. A polling
    subscription's typed take is `rclc_polling_subscription_take(&sub, &msg,
    NULL)`.
  - Generated service helpers keep their shape under the new names
    (`<pkg>__srv__<Name>__service_init`, `__executor_add_service`,
    `__service_handler_t`, …); their handler structs now carry the payloads'
    `__Storage`.
- C API: the C user API takes rcl/rclc names and contracts (RFC-0089,
  phase-492). There are no forwarders; port C code with the rename map
  `docs/reference/c-api-rename-map.tsv` (old name, new name, class, why).

  - A name upstream has takes upstream's spelling and contract
    (`rclc_executor_add_subscription`, `rcl_publish`, `rclc_executor_add_client`, …).
  - A name only nano-ros has moves from `nros_`/`NROS_` to `rclc_`/`RCLC_`
    (`rclc_monitors_arm`, `RCLC_QOS_DURABILITY_TRANSIENT_LOCAL`, …). Return codes
    are `RCLC_RET_*`.
  - Kept as `nros_`: the port seams, the entry and boot hooks (`nros_app_main`,
    `NROS_APP_*`), the build facts, and the C++ binding's `nros_cpp_*`.
  - Headers are reachable at upstream's paths (`<rcl/rcl.h>`, `<rclc/executor.h>`,
    `<rcl_action/*.h>`, `<rcl_lifecycle/*.h>`); `<nros/*.h>` still works.
  - A C component (`RCLC_C_COMPONENT`) is configured with `(rcl_node_t *,
    rclc_executor_t *, StructT *)` and uses the C API; the C and C++ bindings
    share one executor object.
  - Codegen version 14: regenerate C message trees (`nros sync`).
- C API: three enums that already carried upstream's type name now take
  upstream's value names too (phase-492). Same numbers; rename in C code:
  `RCLC_CLOCK_{UNINITIALIZED,ROS_TIME,SYSTEM_TIME,STEADY_TIME}` ->
  `RCL_CLOCK_UNINITIALIZED`, `RCL_ROS_TIME`, `RCL_SYSTEM_TIME`, `RCL_STEADY_TIME`;
  `RCLC_SEMANTICS_RCLCPP_EXECUTOR` / `RCLC_SEMANTICS_LOGICAL_EXECUTION_TIME` ->
  `RCLCPP_EXECUTOR` / `LET`; `RCLC_EXECUTOR_ON_NEW_DATA` / `RCLC_EXECUTOR_ALWAYS`
  -> `ON_NEW_DATA` / `ALWAYS`. The C++ binding's `rclcpp::Clock` takes
  `RCL_ROS_TIME` etc., as upstream rclcpp does.
- Every deprecated item is deleted, in one batch. Each was a forwarder to a
  name that has existed for at least one release; code that still uses an old
  name now fails to compile instead of warning. Replace as follows.

  **C++** (`nros-cpp`):

  | deleted | write instead |
  | --- | --- |
  | `nros::Node` | `rclcpp::Node` (the same type) |
  | `nros::Expected<T>` | `nros::ResultOf<T>` |
  | `nros::bind_timer<C, &C::m>(node, out, ms, self)` | `node.create_wall_timer<C, &C::m>(out, ms, self)` |
  | `Executor::spin(duration_ms, poll_ms)` | `Executor::spin_for(duration_ms, poll_ms)` |
  | `LifecycleNode::trigger(id)` | `LifecycleNode::trigger_transition(id)` |
  | `PollClient::server_available()` | `PollClient::service_is_ready()` (returns `ResultOf<bool>`) |
  | `PollService::try_recv_request(...)` | `PollService::take_request(...)` |
  | `PollService::send_reply(...)` | `PollService::send_response(...)` |
  | `PollSubscription::try_recv(...)` | `PollSubscription::take(...)` |
  | `PollSubscription::try_recv_sized<N>(...)` | `take_sized<N>(...)` |
  | `PollSubscription::try_recv_validated(...)` | `take_validated(...)` |
  | `PollSubscription::try_recv_validated_sized<N>(...)` | `take_validated_sized<N>(...)` |
  | `PollSubscription::try_recv_raw(...)` | `take_serialized(...)` |
  | `PollSubscription::try_recv_raw_with_attachment(...)` | `take_serialized_with_attachment(...)` |
  | `PollSubscription::try_recv_sequence(...)` | `take_sequence(...)` |
  | `QoS::Liveliness` | `nros::LivelinessPolicy` |
  | `QoS::LivelinessNone` / `Automatic` / `ManualByTopic` / `ManualByNode` | `nros::LivelinessNone` / … (namespace scope) |
  | `QoS::reliability_raw()` / `durability_raw()` / `history_raw()` / `liveliness_raw()` | `QoS::reliability()` / `durability()` / `history()` / `liveliness()` (return the policy enums) |
  | `QoS::deadline_ms()` / `lifespan_ms()` / `liveliness_lease_ms()` | `QoS::deadline()` / `lifespan()` / `liveliness_lease_duration()` (return `nros::Duration`) |
  | `QoS::deadline_ms(ms)` / `lifespan_ms(ms)` / `liveliness_lease_ms(ms)` | the same setters taking `nros::Duration` |

  **Rust**:

  | deleted | write instead |
  | --- | --- |
  | `nros::LogCrateLogger`, `nros::LogCrateOnceFlag` | `nros::Logger` with the `nros_*!` macros; `nros::ThrottleState` |
  | `NodeContext::create_node_with_options(opts)` | `create_node(opts)` |
  | `NodeCtx::create_subscription_borrowed` | `create_subscription_viewable` |
  | `Executor::halt()` | `Executor::cancel()` |
  | `ActionServerHandle::cancel(...)` | `ActionServerHandle::canceled(...)` |
  | `SchedClass::TimeTriggered` | a `SchedContext` with `tt_window_offset_us` / `tt_window_duration_us` |
  | `BoardConfig::zenoh_locator()`, `ThreadxConfig::zenoh_locator()` | `locator()` |
  | `Config::with_zenoh_locator(...)` on the five board crates | `with_locator(...)` |

  **C ABI**: `nros_rmw_cffi_register(vtable)` is deleted; a custom RMW backend
  calls `nros_rmw_cffi_register_named("<name>", vtable)`, which every in-tree
  backend already did. The `NROS_DEPRECATED` / `NROS_DEPRECATED_MSG` macros in
  `<nros/visibility.h>` are gone too; nothing used them.
- In Rust, `nros::Node` is now the node itself, with the methods rclrs's `Node`
  has, and `Executor::create_node(name)` returns it. Before, `create_node`
  returned a `NodeHandle` with a different method set, and `nros::Node` was the
  trait a component implements.

  - The component trait is now `nros::Component`. Write
    `impl nros::Component for MyNode`.
  - `nros::NodeCtx` is `nros::Node`, and `nros::NodeHandle` is no longer exported.
  - A node created with `create_node` gets `create_subscription(topic, callback)`
    and `create_service(name, callback)`, which take a callback as rclrs's do. The
    polled forms that `NodeHandle` had under those names are now
    `create_polling_subscription*` and `create_polling_service*`.
- The 32 old build-knob env spellings (`ZPICO_MAX_QUERYABLES`, `ZPICO_FRAG_MAX_SIZE`, the `ZPICO_SMOLTCP_*` family, `ZPICO_READ_TASK_PRIORITY`, ...) are retired. Release 0.5.0-nros2 still read them with a deprecation warning. Now a build that finds one set stops with an error naming the replacement (`ZPICO_MAX_QUERYABLES is a retired spelling of NROS_ZENOH_MAX_QUERYABLES ... -- set NROS_ZENOH_MAX_QUERYABLES instead`). Where it can be set:

  - exported in the environment of a cargo build, a cmake configure or a west build;
  - as an `[image.<id>] env` row in `system.toml`;
  - in a cargo `[env]` table.

  This is true even when the canonical spelling is set to the same value. Rename each one to its `NROS_*` name. The book's deprecated-names page lists all 32. The `ZPICO_*` words remain the zenoh-pico shim's own C macro names, which are generated from the knob and are not user inputs.

  A board descriptor's per-board `[board.knobs.<group>]` table is retired the same way. A descriptor that still states it no longer loads, and the error names the top-level `[knobs.<group>]` table to move it to.
- The C++ API no longer has an `nros::` namespace. Every user-facing name is
  defined in `rclcpp::`, `rclcpp_action::` or `rclcpp_lifecycle::`, where ROS 2
  keeps it, and there is no `nros::` alias to fall back on. Spell
  `nros::Publisher<M>` as `rclcpp::Publisher<M>`, `nros::create_node` as
  `rclcpp::create_node`, `nros::GoalResponse` as `rclcpp_action::GoalResponse`
  and `nros::State` as `rclcpp_lifecycle::State`.

  A few names changed shape as well as namespace, to match upstream:

  - The QoS policy enums are `enum class`: write
    `rclcpp::ReliabilityPolicy::Reliable`, not `nros::Reliable`, and
    `rclcpp::LivelinessPolicy::SystemDefault` for what was `LivelinessNone`.
  - `nros::init(locator, domain)`, `nros::shutdown()` and `nros::spin()` return
    a `Result`, where upstream's `rclcpp::init`, `rclcpp::shutdown` and
    `rclcpp::spin` do not, so they are `rclcpp::init_in(...)`,
    `rclcpp::shutdown_in()` and `rclcpp::spin_in(...)`.
  - The deprecated `nros::LifecycleNode` is gone; derive from
    `rclcpp_lifecycle::LifecycleNode`.

  Generated C++ message headers name the new spellings, so the codegen version
  is 10 and older generated trees must be regenerated (`nros sync`).
- The C++ message FFI view deserializer is renamed to match the `view` storage
  mode: `nros_cpp_deserialize_<pkg>_msg_<type>_borrowed` is now
  `nros_cpp_deserialize_<pkg>_msg_<type>_view`. The generated header and the
  generated Rust export both use the new name, so regenerated bindings need no
  change; only code that names the symbol directly must be updated.
- The `NANO_ROS_*` cmake names are retired. They were deprecated aliases of the
  `NROS_*` names for one release; a configure that sets one now stops with an
  error naming the replacement, instead of adopting it with a warning. This
  covers a `-D` on the command line, a `set()` in a `CMakeLists.txt`, and an
  entry left in a build directory's `CMakeCache.txt` by an older release (the
  error prints the `cmake -U …` that drops exactly those).

  | retired | write instead |
  | --- | --- |
  | `NANO_ROS_PLATFORM` | `NROS_PLATFORM` |
  | `NANO_ROS_FEATURES` | `NROS_FEATURES` |
  | `NANO_ROS_SAFETY_E2E` | `NROS_SAFETY_E2E` |
  | `NANO_ROS_ROS_EDITION` | `NROS_ROS_EDITION` |
  | `NANO_ROS_GEN_CACHE_DIR` | `NROS_GEN_CACHE_DIR` |
  | `NANO_ROS_SKIP_BOOTSTRAP` | `NROS_SKIP_BOOTSTRAP` |
  | `NANO_ROS_BUILD_CODEGEN` | `NROS_BUILD_CODEGEN` |
  | `NANO_ROS_CMAKE_DIR` | `NROS_CMAKE_DIR` |
  | `NANO_ROS_RMW` | `NROS_RMW` |
  | `NANO_ROS_BOARD` | `NROS_BOARD` |
  | `NANO_ROS_ROOT` (cmake variable) | `NROS_REPO_DIR` |
  | `NANO_ROS_ROOT <path>` keyword of `nano_ros_workspace()` and friends | `NROS_REPO_DIR <path>` |

  `nano_ros_DIR` and `nano_ros_ROOT` (CMake's `find_package` hints) are
  unchanged. The error table is kept for one more release; after that an old
  name is an unknown variable. See the book's "Retired names" page.
- The parameter store is now carved from the executor's backing whenever the build knows the image builds one. That happens when the contract declares a parameter, when the bringup declares `param_services` (Zephyr), or when a launch `<param>` seeds one (Zephyr, or anywhere with `NROS_PARAM_STORE=1`). The store leaves the RTOS heap (FreeRTOS heap_4, the Zephyr nros heap, the ThreadX byte pool) and becomes linker-visible `.bss`. An image that builds no store keeps its backing byte for byte. The Zephyr `NROS_ZEPHYR_HEAP_SIZE` default for a `param_services` image drops from 512 KiB to 192 KiB. `ExecutorSizing` gained a `params` field, so a struct literal must name it or use `..ExecutorSizing::DEFAULT`.
- Three names deprecated in `0.5.0-nros2` are **retired** (phase-484 W8c). Each
  is now an error naming its replacement, never silently ignored:

  - **`ZENOH_LOCATOR` / `ZENOH_MODE`** (runtime env) — use `NROS_LOCATOR` /
    `NROS_SESSION_MODE`. A process that still sets either refuses to start:
    Rust's `resolve_hosted` / `ExecutorConfig::from_env` panic and `nros::init`
    returns `InitError::RetiredEnvVar`; the C and C++ init entries return
    `RCL_RET_INVALID_ARGUMENT` / `NROS_CPP_RET_INVALID_ARGUMENT` and log the
    reason at error severity. Ignoring the variable would have been worse: the
    locator it carried would be dropped and the node would dial the default
    router, which reads as "no delivery". Presence alone refuses (an empty value
    too), including beside a canonical value. The native `just zenohd` recipe and
    the `scripts/debug/` helpers read `NROS_LOCATOR` now.
  - **`nros::main!(board = X)`** — a `compile_error!`. Write `nros::main!()` and
    state the board in the leaf's `system.toml` (`[image.<id>] board`); a board
    crate outside the in-tree table states its own type in its manifest,
    `[package.metadata.nros.board] type = "::my_board::MyBoard"`.
  - **`--features rmw-xrce`** on the three PX4 companion examples — the inert
    feature is gone (the xrce backend has been unconditional since phase-277),
    and `just px4` no longer passes it.
- `cmake/compat/` is gone. What it held is now part of nano-ros's own CMake
  package, under new paths:

  - `cmake/compat/NrosRclcppCompat.cmake` → `cmake/NanoRosAmentSurface.cmake`
    (the `ament_*` and `rclcpp_components_register_node` verbs);
  - `cmake/compat/stubs/` → `cmake/find/` (the `Find<pkg>.cmake` modules);
  - `cmake/compat/diagnostic-updater/` → `packages/api/nros-diagnostic-updater/`;
  - `<rclcpp/rclcpp.hpp>` and `<rclcpp_components/register_node_macro.hpp>` are
    nros-cpp headers, on the include path of every C++ target that links
    nano-ros.

  **If your `CMakeLists.txt` includes `cmake/compat/NrosRclcppCompat.cmake`,
  change the path to `cmake/NanoRosAmentSurface.cmake`.** A project that reaches
  nano-ros through `find_package(nano_ros)` or a nano-ros workspace changes
  nothing. `nros/rclcpp_components_compat.hpp` is deleted; include
  `<rclcpp_components/register_node_macro.hpp>`, as upstream code already does.
- `nros-codegen.toml` no longer accepts the storage-mode tokens phase-390
  renamed. `mode = "owned"` and `mode = "borrowed"` parsed with a deprecation
  warning for one release; they are now refused, and code generation stops with
  an error naming the file, the field and the replacement:

  | retired | write instead |
  | --- | --- |
  | `mode = "owned"` | `mode = "inline"` |
  | `mode = "borrowed"` | `mode = "view"` |

  A config that already uses `inline` / `heap` / `view` is unaffected. The
  single-package `cargo nano-ros` path also stops on a config it cannot read,
  where it previously fell back to built-in capacities without saying so.

### Documentation

- A node package in the example workspaces and templates now documents what the
  node DOES, and never which backend implements it underneath. The facts that
  moved are the ones a reader acts on: the `safety` workspace's CRC behaviour and
  the fact that only the zenoh backend implements it today are stated in
  `src/demo_bringup/system.toml` and the workspace README, beside the
  `[system].features = ["safety"]` and `rmw = "zenoh"` lines that cause them, and
  the `bridge-cyclonedds` / `bridge-xrce` talkers point at their bringups'
  `[[domain]]` / `[[bridge]]` rows instead of restating them.

  If you copy `examples/templates/local-msg-package`, the run instructions have
  moved from `src/rust_consumer/src/main.rs` into the template's README and now
  say which RMW they assume: that template declares `rmw = "zenoh"`, so a router
  has to be up, while `nros new --workspace` defaults to `--rmw cyclonedds`, which
  needs none. The module doc used to give the zenoh router command with no such
  qualification. ([#1509](https://github.com/NEWSLabNTU/nano-ros/issues/1509))
