# Open issues

Generated from the issue files — do not hand-edit. Add or resolve an issue by
adding/moving its `NNNN-slug.md`, then run `scripts/gen-issue-index.py`.

This file is `merge=union` (see `.gitattributes`): two agents filing different
issues concurrently both land, instead of colliding in a shared registry. Union
is safe here ONLY because the file is entirely generated and the generator
re-sorts and de-duplicates; `check-issue-index` is the backstop that catches any
residue a union leaves behind.

<!-- BEGIN GENERATED open-issue list — scripts/gen-issue-index.py -->

One line each — the detail lives in the issue file, which already has
it. Regenerate with `scripts/gen-issue-index.py`; `check-issue-index`
fails if this block drifts.

- **#0259** (orchestration) — Derived scheduling is quantitatively inert — no WCET in the model, so blocking is unmodelled and budget/placement/non_preempt can't be derived See `0259-*`.
- **#0506** (embedded) — Transport tasks above application tiers is the right default but has no budget — inbound overload preempts every tier for ~200 ms bursts See `0506-*`.
- **#0736** (core, platform, testing) — `realtime_tiers` nuttx-arm/rust: the fast tier now outruns the slow one but only ~2.4x against a 3x bar — was a 10x inversion, now a marginal shortfall See `0736-*`.
- **#0758** (core, boards) — No platform wall-clock epoch source — embedded consumers hand-roll SNTP before boot, and stamped messages are wrong until they do See `0758-*`.
- **#0760** (orchestration, rmw) — RFC-0074's `ingress` declaration is a ros-launch-manifest schema change, not a nano-ros one — park it until that discussion happens See `0760-*`.
- **#0772** (platform, boards) — FreeRTOS/lwIP has no wall-clock epoch — the same consumer that drove the Zephyr one runs a FreeRTOS board too See `0772-*`.
- **#0783** (api, docs) — `RclReturnCode` exists and is unreachable, and RFC-0036 documents a Rust error type the user API never returns See `0783-*`.
- **#0784** (api) — `nros::` publishes three different audiences under one namespace — the component API a user writes, the machinery `nros::node!` expands into, and four types nothing consumes See `0784-*`.
- **#0788** (api) — The same API verb is spelled differently in our C, C++ and Rust — and in two cases one language ships both spellings See `0788-*`.
- **#0793** (api, params) — C ships two disjoint parameter stores — parameters declared on the node-local one are invisible to `ros2 param`, and its accept/reject callback fires for nobody See `0793-*`.
- **#0794** (build, codegen, boot) — The baked boot config carries four fields and the C/C++ emitter sets one — a launch-declared namespace, domain or locator never reaches a C image See `0794-*`.
- **#0798** (examples) — `examples/workspaces/c`'s root routes `s32z270-freertos` to an entry that hardcodes `mps2-an385-freertos` — the pairing fails all three arms of `_nra_board_active`, so the image links without its platform glue See `0798-*`.
- **#0809** (build) — `provider_scan` honours `NROS_IGNORE` while `nros-pkg-index` honours `.nros-ignore` — the only spelling that exists on disk is the one the order-walk ignores See `0809-*`.
- **#0810** (core) — The executor arena is sized at MAX_CBS x sizeof(ActionClient) whatever the entries actually are, so every real image ships a hand-picked override and undersizing fails at runtime instead of at link See `0810-*`.
- **#0814** (rmw) — The whole zero-copy surface sits behind `feature = \"lending\"`, which only a posix test crate ever enables See `0814-*`.
- **#0815** (tooling) — The static-pool inventory finds 46 sizing knobs and can price 3, so the largest pools in a real image carry no byte figure See `0815-*`.
- **#0816** (tooling) — The book promises no-alloc integrations and nothing checks the linked image, so it is a claim rather than a property See `0816-*`.
- **#0820** (cmake, testing) — `c_riscv_nuttx_e2e` failed on a MUSEUM BINARY — the NuttX seam had no dependency edge on the Rust world, and hardcoded `--release` past a miscompile carve-out See `0820-*`.
- **#0827** (rmw) — Static RAM is a property of the RMW, not of the node — a talker reserves 275 KB of service and large-payload pools it can never reach See `0827-*`.
- **#0830** (boards) — A QEMU net hub with only a NIC and a tap never delivers host->guest frames — OUR lan9118 can_receive patch deadlocks before the guest enables RX See `0830-*`.
- **#0831** (build) — `[image.<id>].rmw` configures nothing on the cargo driver — and a workspace fixture row's `rmw` does not either, so two tier-2 coordinates test zenoh while claiming cyclonedds and XRCE See `0831-*`.
- **#0835** (testing) — The cmake and rust fixture families re-stale each other, so `check-fixtures-stale` never reaches a fixed point and `just ci-matrix` fails ~190 tests on every run See `0835-*`.
- **#0849** (cli) — `nros sync` bakes the invocation's path SPELLING into every leaf patch table, so working through a symlink to the checkout makes cargo see two copies of every core crate and refuse the build on `links` See `0849-*`.
- **#0852** (rmw, platform) — the zenoh read task inherits the executor's priority on Zephyr — the declared priority is discarded by the port, so a 20 ms timeslice starves the polled serial RX and it overruns See `0852-*`.
- **#0854** (testing) — `action_raw_goal_ships_one_cdr_header` times out in-sweep and passes solo with a 16x margin — starved, not slow See `0854-*`.
- **#0857** (api) — ComponentCell's inline registries cost worst-case × biggest-payload heap per component See `0857-*`.
- **#0865** (examples, docs) — parameter services are implemented and tested but undiscoverable: no example calls them, and the C header declares the entry point unconditionally so a caller without the feature gets a bare `undefined reference` See `0865-*`.
- **#0870** (rmw, examples) — NuttX C++ action client fails `create_action_client` — the session reports `Transport(ConnectionFailed)` against a router the server reached See `0870-*`.
- **#0871** (ci, testing) — Every PR is red on a fixture CI never builds — and `main` cannot see it, because the required gate does not run on push See `0871-*`.
- **#0872** (ci) — The PR/nightly check arm has never run to completion — each fix exposes the next environment gap See `0872-*`.
- **#0874** (ci, tooling) — sccache 0.8.2 speaks a GitHub cache API that no longer exists — and because it is the `RUSTC_WRAPPER`, that fails every `rustc` See `0874-*`.
- **#0880** (platform, embedded) — 192 KiB of tightly-coupled memory sits at 0 % while SRAM is exhausted — the Zephyr images place nothing in ITCM or DTCM See `0880-*`.
- **#0895** (build) — `just format` is red or green depending on whether a migrated colcon workspace has been BUILT See `0895-*`.
- **#0900** (core, memory) — Every executor arena slot is budgeted at the ActionClient worst case, so a pub/sub-only image carries ~56 KiB it cannot use See `0900-*`.
- **#0902** (rmw) — action goals complete between 20 % and 90 % of the time on the same build, with no session expiry and no fault to explain the difference See `0902-*`.
- **#0910** (rmw, build) — migrating to zenoh-pico 1.10: the serial layer moved, `config.h` is no longer shipped, and our config generator is 54 knobs behind See `0910-*`.
- **#0913** (testing, embedded) — attaching pyocd RTT kills the zenoh session — the debugger perturbs the system under test, and issue 0879 makes the perturbation permanent See `0913-*`.
- **#0914** (testing) — Nothing exercises the SHIPPED resolver + `pyexec` pair, so a resolver that cannot evaluate anything passes every check See `0914-*`.
- **#0917** (rmw, platform) — The emulated LAN9118 RX FIFO cannot hold an 8-fragment RTPS burst, and a 5 ms RX poll drains it far too late See `0917-*`.
- **#0925** (tooling) — `ros2-box-sync.sh` copies the GENERATED workspace manifests while excluding the `build/` members they list, so every box fixture build dies in `cargo metadata` See `0925-*`.
- **#0930** (testing, tooling) — The built QEMU can be older than the commit `third-party/qemu/qemu` pins, and nothing says so See `0930-*`.
- **#0939** (tooling) — The metadata probe links the node NAME, which is not a target — so every C/C++ component reports `no producer`, once, and then silently See `0939-*`.
- **#0945** (build) — The shared-cargo-dir campaign rests on five unsupported build-system internals — a Corrosion path formula, an unstable cargo flag, cargo's private `.fingerprint` format, a side channel inside cargo's target dir, and an undocumented depfile location See `0945-*`.
- **#0953** (tooling) — A panic in the Python half crosses `extern \"C\"` and ABORTS the resolver — 0897 removed the loader abort and left this one See `0953-*`.
- **#0954** (api-c, boards) — The committed NuttX fallback sizes header is a hand-maintained twin with no gate, and went stale again See `0954-*`.
- **#0957** (build) — `just format` fails whole-tree — a workspace leaf is in neither the root members nor the exclude list See `0957-*`.
- **#0961** (core, memory) — Executor::open_in needs more than 32 KiB of the calling thread's stack, and nothing says so See `0961-*`.
- **#0963** (codegen, memory, build) — The derived-bound inventory has readers now — what is left is the executor arena alone (was: nothing reads it) See `0963-*`.
- **#0964** (codegen) — The C++ header states an ESTIMATED size for every type, including types that have no bound See `0964-*`.
- **#0965** (codegen, memory, build) — Nothing states which entities an image creates, so the arena, MAX_CBS and the payload classes stay hand-set See `0965-*`.
- **#0968** (testing) — Tier 2 has ~12 runtime e2e failures on main, unreproduced — nobody has run the tier in a long time See `0968-*`.
- **#0969** ([rmw, memory]) — The Cyclone RMW deserializes every received sample and re-serializes it, so `take_serialized` costs a decode, an encode and two heap allocations per take See `0969-*`.
- **#0973** (orchestration) — No resolved SystemModel describes endpoint wiring — 0 of 119 carry topics, services or actions See `0973-*`.
- **#0976** ([rmw, testing]) — Five action adapters in the Cyclone service path reshape the CDR to match ROS 2, and the only thing that exercises them is nano-ros talking to itself See `0976-*`.
- **#0986** (tooling) — The pre-push hook writes into the repository it is guarding See `0986-*`.
- **#0988** (tooling) — No gate runs a hook the way git runs it — with `GIT_DIR` set — so a script that corrupts the caller's repository passes every check See `0988-*`.
- **#0996** (ci, tooling) — CI audit: the same lane passes in the queue and fails after merge, four of six lanes carry no signal, and the provisioning system is bypassed by hand See `0996-*`.
- **#0997** ([rmw, platform, embedded]) — The timed-event tree empties itself on FreeRTOS: the SPDP resend is scheduled, never lands in the queue, and every peer expires the participant's lease See `0997-*`.
- **#1000** ([rmw, third-party]) — Vendored Cyclone: `handle_xevk_spdp`'s early returns orphan the PERIODIC spdp event — it leaves the heap, is never re-armed, and the participant goes silent forever See `1000-*`.
- **#1001** (tooling, ci) — `check-action-client-arena-budget` walks the whole repo, so `check fast` costs minutes on a cold page cache — the pre-push gate is where that is felt See `1001-*`.
- **#1002** (cmake) — A derived knob converges after THREE configures, not the two 0991 documents See `1002-*`.
- **#1004** ([rmw, platform, embedded]) — an536 boot failures on this host were HOST LOAD, not a code regression — and the measurements taken during them should not be trusted See `1004-*`.
- **#1005** (testing, build) — A zenoh constant that lives in `nros-zpico-build` is invisible to the fixture staleness probe, so a fixture baked before a fix reports FRESH See `1005-*`.
- **#1006** (tooling, build) — esp32-qemu's configure does not disable the backends it never uses, so its runtime dependency set is a property of the machine that built it See `1006-*`.
- **#1007** (testing, build) — `just nuttx build-fixtures-arm` can leave every arm cell unrunnable, and the remedy it prints is the command that just short-circuited See `1007-*`.
- **#1008** (api, rmw) — `wait_for_service` returns `Ok(true)` immediately on every real backend — its fast path calls `is_server_ready()`, whose trait default is `true` and which only zenoh overrides See `1008-*`.
- **#1009** (testing, ci) — Our DDS interop tests share a bus with the whole LAN, so a foreign peer on another host can fail them — and `ROS_LOCALHOST_ONLY=1` alone does NOT fix it See `1009-*`.
- **#1012** (docs, api) — Parity-ledger `why` prose names symbols a rename retired — 15 rows describe current state using dead spellings, and nothing checks it See `1012-*`.
- **#1013** (testing) — `test_rtos_pubsub_e2e` SIGKILLs its talker after ~12 publishes, so the cell exercises twelve seconds of a free-running publisher See `1013-*`.
- **#1018** (build, codegen) — A codegen change invalidates every consumer's generated interfaces, and only a manual `setup-cli` connects them See `1018-*`.
- **#1019** (api, docs) — Every `RCLCPP_*` log call in a ported C++ node is discarded on embedded, and `RCLCPP_*_STREAM` drops its message on every target See `1019-*`.
- **#1020** (docs, api) — The C++ parity lane measures the NATIVE API against rclcpp and cannot see `rclcpp_compat.hpp` — 589 lines whose entire purpose is the thing being measured See `1020-*`.
- **#1023** (rmw, build) — `nros_sertype.cpp` includes `<memory>` and `<string>`, so cyclonedds cannot compile for a freestanding target See `1023-*`.
- **#1025** (build, testing) — ESP32 flash images can never be built: the packer asks for the group dir with the row's env stripped, so it looks in a directory the build stopped using See `1025-*`.

<!-- END GENERATED open-issue list -->
