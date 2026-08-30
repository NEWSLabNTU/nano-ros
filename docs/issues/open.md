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
- **#0741** (rmw, testing) — `test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the 28-byte reply into a 15-byte history payload See `0741-*`.
- **#0758** (core, boards) — No platform wall-clock epoch source — embedded consumers hand-roll SNTP before boot, and stamped messages are wrong until they do See `0758-*`.
- **#0760** (orchestration, rmw) — RFC-0074's `ingress` declaration is a ros-launch-manifest schema change, not a nano-ros one — park it until that discussion happens See `0760-*`.
- **#0772** (platform, boards) — FreeRTOS/lwIP has no wall-clock epoch — the same consumer that drove the Zephyr one runs a FreeRTOS board too See `0772-*`.
- **#0783** (api, docs) — `RclReturnCode` exists and is unreachable, and RFC-0036 documents a Rust error type the user API never returns See `0783-*`.
- **#0784** (api) — `nros::` publishes three different audiences under one namespace — the component API a user writes, the machinery `nros::node!` expands into, and four types nothing consumes See `0784-*`.
- **#0788** (api) — The same API verb is spelled differently in our C, C++ and Rust — and in two cases one language ships both spellings See `0788-*`.
- **#0791** (api, rmw) — We are visible in the ROS graph and cannot read it — 12 rmw vtable graph slots exist, all `None`, while both backends already run the discovery machinery See `0791-*`.
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
- **#0828** (testing) — Tier 2 RUNS rows its build lane never builds, so `just ci-matrix` is green only while an earlier `lane=all` build is still fresh See `0828-*`.
- **#0829** (api, rmw) — Two `SYSTEM_DEFAULT` QoS presets ship under one meaning and disagree on depth — 1 in `nros-rmw`, 10 in the `nros::qos` façade, each with two callers See `0829-*`.
- **#0830** (boards) — A QEMU net hub with only a NIC and a tap never delivers host->guest frames — OUR lan9118 can_receive patch deadlocks before the guest enables RX See `0830-*`.
- **#0831** (build) — `[image.<id>].rmw` configures nothing on the cargo driver — and a workspace fixture row's `rmw` does not either, so two tier-2 coordinates test zenoh while claiming cyclonedds and XRCE See `0831-*`.
- **#0834** (cmake) — The per-build `nros_cpp_config_generated.h` mirror can reach a state no re-run repairs — only wiping the west build dir recovers it See `0834-*`.
- **#0835** (testing) — The cmake and rust fixture families re-stale each other, so `check-fixtures-stale` never reaches a fixed point and `just ci-matrix` fails ~190 tests on every run See `0835-*`.
- **#0841** (rmw) — A subscription whose hint lands between the small block size and the size threshold gets a block that cannot hold it — and the build error's own remedy puts it there See `0841-*`.
- **#0847** (rmw) — An XRCE publisher outliving `executor.close()` segfaults in its own Drop: the entity destructor dereferences the session state that close already freed See `0847-*`.
- **#0849** (cli) — `nros sync` bakes the invocation's path SPELLING into every leaf patch table, so working through a symlink to the checkout makes cargo see two copies of every core crate and refuse the build on `links` See `0849-*`.
- **#0852** (rmw, platform) — the zenoh read task inherits the executor's priority on Zephyr — the declared priority is discarded by the port, so a 20 ms timeslice starves the polled serial RX and it overruns See `0852-*`.
- **#0854** (testing) — `action_raw_goal_ships_one_cdr_header` times out in-sweep and passes solo with a 16x margin — starved, not slow See `0854-*`.
- **#0857** (api) — ComponentCell's inline registries cost worst-case × biggest-payload heap per component See `0857-*`.
- **#0865** (examples, docs) — parameter services are implemented and tested but undiscoverable: no example calls them, and the C header declares the entry point unconditionally so a caller without the feature gets a bare `undefined reference` See `0865-*`.
- **#0867** (testing, rmw) — `test_rtos_action_e2e` nuttx/C fails 3/3 SOLO — the client's goal send times out (-2) against a server sitting at its ready banner See `0867-*`.
- **#0868** (examples, testing) — A `send_goal` TIMEOUT prints as `Goal was rejected by server`, so an intermittent XRCE action failure reads as a deterministic server decision See `0868-*`.
- **#0870** (rmw, examples) — NuttX C++ action client fails `create_action_client` — the session reports `Transport(ConnectionFailed)` against a router the server reached See `0870-*`.
- **#0871** (ci, testing) — Every PR is red on a fixture CI never builds — and `main` cannot see it, because the required gate does not run on push See `0871-*`.
- **#0872** (ci) — The PR/nightly check arm has never run to completion — each fix exposes the next environment gap See `0872-*`.
- **#0874** (ci, tooling) — sccache 0.8.2 speaks a GitHub cache API that no longer exists — and because it is the `RUSTC_WRAPPER`, that fails every `rustc` See `0874-*`.
- **#0877** (testing, boards) — FreeRTOS pubsub delivers by hand and delivers NOTHING under the test harness — and the talker trips a FreeRTOS queue assert See `0877-*`.
- **#0880** (platform, embedded) — 192 KiB of tightly-coupled memory sits at 0 % while SRAM is exhausted — the Zephyr images place nothing in ITCM or DTCM See `0880-*`.
- **#0881** (testing, embedded) — attaching pyocd RTT kills the zenoh session — the debugger perturbs the system under test, and issue 0879 makes the perturbation permanent See `0881-*`.
- **#0891** (testing) — Six nuttx `rtos_e2e` cells fail in-sweep and pass solo — the group cap was never measured, and a slow boot is reported as a dead image See `0891-*`.
- **#0895** (build) — `just format` is red or green depending on whether a migrated colcon workspace has been BUILT See `0895-*`.
- **#0896** (rmw, api) — Every C/C++ subscription takes the small size class regardless of its message type — nothing fills `rx_buffer_hint` See `0896-*`.
- **#0897** (tooling) — `nros-launch-resolve` hard-links one `libpython` soname, so one build serves one interpreter — and abi3, which issue 0400 recommends, does not apply to embedding See `0897-*`.
- **#0899** (rmw, boards) — The FreeRTOS C talker dies mid-run inside zenoh-pico's write buffer — two different asserts, both after tens of successful publishes See `0899-*`.
- **#0900** (core, memory) — Every executor arena slot is budgeted at the ActionClient worst case, so a pub/sub-only image carries ~56 KiB it cannot use See `0900-*`.
- **#0902** (rmw) — action goals complete between 20 % and 90 % of the time on the same build, with no session expiry and no fault to explain the difference See `0902-*`.
- **#0902** (build, rmw) — Editing zenoh-pico rebuilds nothing — `zpico-sys` watches 7 hand-listed files out of the whole library, so a patch is silently not compiled See `0902-*`.
- **#0903** (rmw) — `get_topic_names_and_types` returns EMPTY against a live rmw_zenoh_cpp node, while `get_node_names` on the same session returns the node See `0903-*`.
- **#0910** (rmw, build) — migrating to zenoh-pico 1.10: the serial layer moved, `config.h` is no longer shipped, and our config generator is 54 knobs behind See `0910-*`.

<!-- END GENERATED open-issue list -->
