# Phase 482 — a ported ROS 2 C++ node builds and runs on every platform, with no compat layer

**Status (2026-10-07). Opened.** Consolidates the open residue of four phases
that each owned a slice of the same acceptance, and archives them:
[phase-209](archived/phase-209-cpp-port-friction-reduction.md) (C++ port
friction), [phase-379](archived/phase-379-api-parity-with-ros2-client-libraries.md)
(API parity measurement),
[phase-417](archived/phase-417-ros2-api-adoption.md) (ROS 2 API adoption) and
[phase-442](archived/phase-442-one-freestanding-rclcpp-api.md) (one freestanding
`rclcpp` API). Implements what is left of
[RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
and [RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md).

## Why one phase

An assessment of the four against the tree on 2026-10-07 (`origin/main`
`edbe96f92`) found each one mostly finished, with docs that had drifted from
the code:

- **Gap counts disagreed with each other and with the ledger.** phase-379 said
  158 and then 149 `gap` rows, phase-417 said 80, phase-467 said 5. The ledger
  holds **1** (`rust:init_with_args`).
- **Port acceptance was stated three times**: 209 G.1, 379 W3, and 417 stages
  1 and 6. It is met on posix only.
- **Ownership was circular in one place and missing in another.** 379 W7
  step 4 and 417 W-R1/W-B6 each said the other owned the retirement of
  deprecated aliases. Nothing owned RFC-0096 D4, the deletion of
  `cmake/compat/`.
- **Lifecycle was described in two phases** (209.H, 417 W-B5) and owned by
  neither.
- **phase-442 said "Opened"** at the top while its body said it was complete.

Every item that remains serves ONE acceptance, so it gets one home.

## Decisions taken when this phase was opened (2026-10-07)

1. **`cmake/compat/` is deleted, per RFC-0096 D4 and RFC-0089 "End state".**
   It is not kept as permanent porting tooling. Whatever a ported
   `CMakeLists.txt` needs from it either becomes part of nano-ros's own CMake
   package, or becomes a stated D5 edit (W1).
2. **`rclcpp::Node::SharedPtr` becomes a freestanding, copyable
   `nros::Handle<Node>`, as RFC-0096 D5 item 4 already says.** Today it is
   `std::shared_ptr<Node>` behind `hosted-family: shared-ptr-interop`
   (`node.hpp`). `std::make_shared<rclcpp::Node>(…)` stays as hosted interop
   over the handle (W2).
3. **Lifecycle adopts `rclcpp_lifecycle::LifecycleNode`.** It becomes a
   `Node`-derived type with upstream's constructor shape, per RFC-0089 "`nros::`
   is phased out; ours-only names take `rclcpp::` too".
   `nros::LifecycleNode(void* executor_handle)` becomes a deprecated forwarder
   for one release (W4).

## Work items

### W1 — delete `cmake/compat/`

What is there today:

- `NrosRclcppCompat.cmake`, included at workspace scope by
  `cmake/NanoRosWorkspace.cmake:397` (phase-445 W5);
- `include/rclcpp/rclcpp.hpp`, which already forwards straight to
  `<nros/nros.hpp>`;
- `include/rclcpp_components/register_node_macro.hpp`;
- `diagnostic-updater/`, a header-only `diagnostic_updater`;
- about 30 `stubs/Find<pkg>.cmake`, for `ament_cmake*`, `rclcpp`, `rcl`, `rmw`,
  the `rosidl_*` packages and the common message packages.

Consumers: the port templates under `examples/templates/`
(`cpp-port-minimal-publisher`, `rclcpp-compat-smoke`,
`topic-state-monitor-port`, `local-msg-package`), and the book's
`porting-a-cpp-node.md`.

- Inventory what each stub and the `.cmake` actually provide to a ported
  `CMakeLists.txt`.
- For each, choose one of two answers:
  - **first-class:** nano-ros's installed CMake package exports it, as
    `rclcppConfig.cmake` and friends. That IS the package, not a compat layer
    over it.
  - **D5 edit:** a ported `CMakeLists.txt` must change one named line, and
    RFC-0096 D5 lists it.
- `<rclcpp/rclcpp.hpp>` and `<rclcpp_components/register_node_macro.hpp>`
  resolve from `packages/api/nros-cpp/include`. `diagnostic_updater` moves to
  a real package under `packages/`.
- **Acceptance:** `git ls-files cmake/compat` is empty. Every port template
  builds and runs. The book describes only what remains.

**Status: done 2026-10-07.** Every piece took the first-class answer; no D5
edit was needed.

- `<rclcpp/rclcpp.hpp>` and `<rclcpp_components/register_node_macro.hpp>`
  moved into `packages/api/nros-cpp/include/`. `nros/rclcpp_components_compat.hpp`,
  the force-include of it and the private include directory are gone.
- `NrosRclcppCompat.cmake` is `cmake/NanoRosAmentSurface.cmake`; the stubs are
  `cmake/find/`; `diagnostic-updater/` is `packages/api/nros-diagnostic-updater/`.
- They stay find MODULES, not the `rclcppConfig.cmake` files this item first
  proposed. `find_package()` tries module mode before config mode, so a module
  is what keeps nano-ros's `rclcpp` ahead of `/opt/ros/<distro>` after
  `setup.bash` is sourced; a config file would lose to it.
- Two consumers derived paths from the old depth and failed OPEN when they
  missed: the message resolver's `EXISTS`-guarded include of
  `NanoRosGenerateInterfaces.cmake`, and the CLI's
  `compat_provided_packages()`, which read an absent directory as "nano-ros
  supplies no packages". The first is corrected; the second is now
  `nano_ros_provided_packages()` and refuses a checkout without `cmake/find/`.

### W2 — `Node::SharedPtr` is a freestanding `Handle<Node>`

- `rclcpp::Node::SharedPtr` / `ConstSharedPtr` alias `nros::Handle<Node>` on
  every target. This is the model of the entity handles of phases 456/476:
  copyable, and no ownership claim beyond what the arena holds.
- `std::make_shared<rclcpp::Node>(…)`, `shared_from_this()` and the
  `SharedPtr`-taking spin verbs stay as hosted interop (the `shared-ptr-interop`
  family in `scripts/check-cpp-hosted-family.py`), converting into the handle.
  If that family has no remaining member, it is deleted.
- `diagnostic_updater::Updater`'s by-value node constructor, which phase-209 D
  measured as the one by-value pass in the corpus, works freestanding.
- **Acceptance:**
  - A compile probe, built without `NROS_CPP_STD`, stores and copies a
    `Node::SharedPtr`.
  - The api-parity rows move from hosted to freestanding.
  - RFC-0096 D5 item 4 needs no amendment.

**Status: done 2026-10-07** (#1772). One consequence became RFC-0096 D5 item
5: `Node::SharedPtr x = std::make_shared<MyNode>(…)` is a compile error, because
the handle cannot own a temporary; `auto` keeps the owner.

### W3 — the port templates run on an RTOS

This carries phase-209 G.2, G.3 and G.4.

- The two-layer port (unchanged node source plus build glue) builds and runs
  on Zephyr native_sim and on FreeRTOS (mps2-an385, QEMU), with an e2e cell
  each.
- The embedded entry is `nros_app_main` plus the configuration bake. G.3 found
  that the compat `.cmake` lacked this; after W1 it lives in the first-class
  package.
- G.4: one larger real-world port fixture, beyond the minimal publisher and
  `topic-state-monitor`.
- **Acceptance:** every port template has a runtime cell on posix, Zephyr and
  FreeRTOS, recorded as `matrix::CELLS` rows.

**Status 2026-10-09: first template done; two templates and G.4 open.**
`cpp-port-minimal-publisher` runs unmodified on all three platforms. Its cells
are `Workload::Port` × {Linux, FreertosMps2, ZephyrQemuCortexM}, run by
`port_templates_e2e`. What it took:

- **`nano_ros_add_executable(... ROS2_MAIN)`** (`cmake/NanoRosVerbs.cmake`).
  The program's `main` is renamed per source to `nros_ported_main`, and a
  generated C++ file defines `nros_app_main`, which forwards to it, and
  registers it. A private name rather than `main=nros_app_main`, because
  Zephyr's `<zephyr/types.h>` declares `int main(void)` and the rename turned
  that into a conflicting second declaration. Per source rather than on the
  target, because the board's startup has its own `main`. On Zephyr the glue
  goes into `app`: the placeholder library `nano_ros_entry` leaves behind is
  not linked whole-archive, so a `nros_app_main` placed there was dropped. On
  FreeRTOS `-ffreestanding` leaves the caller's directory (`-fhosted` is
  C-only), because a hosted libstdc++ is an `#error` under it.
- **`<nros/node.hpp>` includes `<nros/entry_config.h>`.** Measured on Zephyr:
  a ported `main.cpp` includes only `<rclcpp/rclcpp.hpp>`, so
  `NROS_ENTRY_LOCATOR` was undefined in the one TU calling `rclcpp::init`. The
  backend then dialled its default, the guest's own loopback, and the node
  aborted at `create_publisher` with `ConnectionFailed`. Every Zephyr image
  whose `init` call sat outside a `<nros/main.hpp>` TU had this. FreeRTOS
  escaped it only because its board bakes the macro as a compile definition.
- **A `DEPLOY zephyr` entry gets `BOARD zephyr`** (`cmake/NanoRosEntry.cmake`).
  `[image.zephyr] board = "zephyr"` names no separate provider, so the entry
  gate refused the one deploy whose board needs no naming.
  `nano_ros_add_executable` could not build a Zephyr leaf at all; the in-tree
  Zephyr leaves register components instead and never reached it.
- **G.2 targets `mps2/an385`, not `native_sim`.** The blocker phase-209
  recorded still holds: a ported program needs the full libstdc++, and
  native_sim's C library cannot carry the host's. The Cortex-M board with the
  SDK's libstdc++ (`CONFIG_GLIBCXX_LIBCPP`) builds and runs. ROS 2 humble's
  `ros2 topic echo` received `Hello, world! 0` from the guest.

**Status 2026-10-10: all three templates done; G.4 open.**
`rclcpp-compat-smoke` and `topic-state-monitor-port` now run unmodified on
posix, FreeRTOS (mps2-an385) and Zephyr (mps2/an385). Their cells are
`Workload::PortSmoke` and `Workload::PortMonitor` × {Linux, FreertosMps2,
ZephyrQemuCortexM}, run by `port_templates_e2e`, so the acceptance line ("every
port template has a runtime cell on posix, Zephyr and FreeRTOS") now holds for
all three. Each is checked from outside, by host peers on the same router, so
no cell needs ROS 2:

- **The smoke node:** a host `int32-sink` must receive 3 samples of
  `/smoke_topic`, and the contract-monitor diagsink must receive its
  `publish_count` diagnostics task at level OK.
- **The monitor:** with nothing publishing, the diagsink must see both topics
  `a` and `b` reported ERROR (stale). Then a new host fixture, `bins/int32-source`,
  publishes `/a` and `/b` every 100 ms, and both must turn OK. The order is the
  assertion: a clock that never advanced would report every topic OK with no
  publisher at all. Mutation: publishing `/mut_a,/mut_b` instead fails the posix
  cell on the second assertion.

What it took:

- **FreeRTOS: the board provides `sleep`, `usleep` and `_gettimeofday`**
  (`nros-board-freertos/c/freertos_c_entry.c`, weak). Both templates' `main`
  loops on `std::this_thread::sleep_for`, which the pinned GCC 13.2's libstdc++
  implements with `sleep`/`usleep`. newlib declares those and defines neither,
  so the image did not link. `std::chrono::steady_clock` is `system_clock` in
  that library (no monotonic clock is configured), which calls `gettimeofday`.
  libnosys's stub fails without writing the result, so `diagnostic_updater`'s
  rate limit and the monitor's age arithmetic read uninitialized time. The
  board has no RTC, so the time is counted from boot. Every FreeRTOS C and C++
  fixture row still builds with the change. The system GCC 10.3 fallback has no
  `std::this_thread` at all without gthreads, so these two templates need the
  pinned toolchain (`nros setup --tool arm-none-eabi-gcc`).
- **`ament_target_dependencies` works on Zephyr and follows the sources to
  `app`.** The verb is now `cmake/NanoRosAmentTargetDeps.cmake`, included by
  the ament surface and by the Zephyr arm of `nano_rosConfig.cmake`, which had
  skipped it. A `nano_ros_entry` placeholder records
  `NROS_SOURCES_IN_TARGET app`, and the verb links there, so a header-only
  dependency such as `diagnostic_updater` reaches the ported sources. The
  FreeRTOS and Zephyr `CMakeLists.txt` are now the same four lines.
- Each sub-project has an `nros-codegen.toml` that bounds `diagnostic_msgs`
  with the core interface set's numbers. Without the bounds, the default 64 ×
  256-byte sequences make a `DiagnosticArray` too large for a
  microcontroller's stack.
- Ports `alloc::port_of(…, PortSmoke)` = offset 88 and `PortMonitor` = 89
  (8088/8089 FreeRTOS, 10888/10889 Zephyr). The `contract-monitor` fixture row
  gained an `id`, so its diagsink can be built alone. Tier 1 now covers 22
  cells. Its coordinates stay at 12.

Measured: `cargo nextest run -p nros-tests --test port_templates_e2e`, 9/9 pass
(the three minimal-publisher cells plus the six new ones), with the fixtures
built through the FreeRTOS cmake lane and the west lane.

Open: G.4, the larger real-world port fixture. It was not attempted here.

### W4 — `rclcpp_lifecycle::LifecycleNode`

- A `Node`-derived `rclcpp_lifecycle::LifecycleNode` with upstream's
  `(name[, options])` constructor, the transition callbacks
  (`on_configure` … `on_shutdown`) and `LifecyclePublisher`.
- `nros::LifecycleNode` is a deprecated forwarder.
- **Acceptance:**
  - A ported lifecycle node's class body compiles unchanged.
  - The ledger's `cpp:LifecycleNode*` rows read `adopt` or `adopt-bounded`.
  - A runtime probe drives configure, activate, deactivate and cleanup.

**Status: done 2026-10-07.**

- `rclcpp_lifecycle::LifecycleNode` derives from `rclcpp::Node` and owns the
  REP-2002 engine, which is now `nros::detail::LifecycleEngine`. The
  `node_interfaces::LifecycleNodeInterface` hooks have upstream's signatures
  and `CallbackReturn` values (97/98/99).
- Include paths `<rclcpp_lifecycle/{lifecycle_node,lifecycle_publisher,state,transition}.hpp>`
  are provided.
- `create_publisher<M>` returns a managed `LifecyclePublisher`. Managed
  entities became MOVABLE so they can live in the move-only `nros::Owned<T>`:
  a move re-points the node's intrusive link, and a destructor unlinks.
- `nros::LifecycleNode` is the deprecated forwarder.
- Acceptance:
  - `rclcpp_lifecycle_ported_node.cpp` (upstream's `lifecycle_talker` class
    body) compiles with and without `NROS_CPP_STD`.
  - The `managed` workspace's `ManagedTalker` runs Configure, Activate,
    Deactivate and Cleanup at boot, and `cpp_lifecycle_node_wrapper_e2e`
    asserts the states `2,3,2,1`.
  - The lifecycle ledger rows were rewritten. The node's own surface reads
    `adopt` / `adopt-bounded`.
  - The ~40 methods it inherits from `rclcpp::Node` mirror `cpp:Node::*`'s
    disposition. They correlate as upstream-only because the extractor records
    declared members, not inherited ones, so a row that is `refuse-loud` on
    `Node` stays without a disposition here. Attributing inherited members is
    an extractor change left for later.

### W5 — `rclcpp::init(argc, argv)`: `-p` and `--params-file`

- `-r` has been honoured since 2026-10-05. The parameter half is the ledger's
  last `gap` (`rust:init_with_args`), and it is the same work in C++ and C.
  Overrides apply to the node's parameter store before the first `declare`.
- **Acceptance:** zero `gap` rows in the ledger, and a test that boots with
  `-p` and with `--params-file`.

**Status: done 2026-10-07.**

- **Parsing.** `nros_node::ros_args` parses `-p` and `--params-file` into events
  (`parse_ros_args_events`). `parse_params_yaml` reads a parameter file's text:
  no `std`, no allocation, and every construct outside the subset is refused
  with its line number.
- **Storage.** The executor holds the overrides in its boxed parameter state
  (`install_argv_params`), so an image without `param-services` pays nothing.
- **Application.** Both declare seams apply the last matching override, typed
  by the declared default. A type mismatch, or a descriptor refusal, is logged
  and the default declared instead.
- **Precedence.** Argv overrides a launch-baked value, which is RFC-0015 §9's
  purpose; remaps go the other way.
- **Hosts.**
  - Rust's `init_with_args` and `Context::new` read files with `std::fs`, carry
    the overrides on the `Context`, and install them in `create_executor`.
  - C++'s `nros_cpp_install_argv_remaps` uses `apply_ros_args_with_params`
    under `param-store`, which is all-or-nothing and refuses files on `no_std`.
  - C has no argv entry point.
- **Acceptance.**
  - `rust:init_with_args` leaves `gap` (`divergence`/`adopt-bounded`, with its
    envelope in the function's doc), and the ledger holds zero `gap` rows.
  - Tests:
    - `nros-node` executor tests boot named nodes with `-p` (last-wins,
      `node:`, wrong type) and with a parameter file (`/**`, a node key,
      dotted names);
    - the `nros` facade tests carry both onto a `Context` from a real file;
    - the `init` lane gains a `param-services` run so those tests run.

### W6 — retire the deprecated aliases in one batch

This carries phase-379 W7 step 4 and phase-417 W-R1/W-B6, ending the circular
ownership.

- Measured 2026-10-07: 32 `[[deprecated]]` in `nros-cpp/include` (16 in
  `qos.hpp`), 17 `#[deprecated]` in Rust (5 `with_zenoh_locator` builders and
  `ThreadxConfig::zenoh_locator` among them), and the `NROS_DEPRECATED_MSG`
  macro in `visibility.h` with no remaining use.
- Delete them in one change, with one `changelog.d/*.breaking.md` entry listing
  every name and its replacement.
- **Acceptance:** zero deprecated items, except W4's lifecycle forwarder,
  which is due for removal in the following release.

**Status: done 2026-10-07.** Measured at deletion: 32 C++ `[[deprecated]]`
plus the two `NROS_CPP_DEPRECATED_MSG` uses (`nros::Node`, `bind_timer`), 16
Rust `#[deprecated]` items, and the two unused C macros. One changelog entry
(`changelog.d/+phase-482-w6.breaking.md`) lists every name and its replacement.

- The five C++ expected-failure deprecation probes went with them, as the C
  ones did in phase-417 W-B5: with the identifier gone, a probe that expects a
  compile failure passes while asserting nothing. Their positive twins keep
  only the live spellings.
- 23 API-parity rows for deleted names are kept, marked RETIRED. Nothing would
  have flagged them (issue 1323, W7).
- `nros_rmw_cffi_register` (the unnamed C ABI entry) is deleted. Its weak
  PX4-SITL fallback in `nros-rmw-uorb/src/register_fallback.c` defined that
  unnamed symbol while `vtable.cpp` calls `nros_rmw_cffi_register_named`, so
  the fallback had satisfied nothing since the named registry landed. It now
  defines the symbol that is called.
- `NROS_CPP_DEPRECATED_MSG` stays defined, unused, for W4's forwarder.

### W7 — ledger hygiene and export policy

- Issue 1323: the API-parity ledger has no stale-row detection. It was homed
  in phase-442.
- Issue 1042: nine ledger rows went false when the rclrs pin moved.
- Issues 1302 and 1303: re-check first. phase-467 cites "#1302/#1303" as PRs
  that settled the matching rows, which may be a PR/issue number collision.
- Issue 1335: the C++ API uses the poll path where it means dispatch. Most of
  it was fixed by phase-456 W3/W3b; close it or re-scope it.
- Issues 0783 and 0784: the facade's export policy (what `nros::` publishes,
  and to which audience).
- Correct the status lines that drifted: RFC-0089 and RFC-0096 were promoted to
  Stable when this phase opened; their D4/D5 text stands as decided above.

**Status 2026-10-07: done, except for three maintainer decisions.**

| Issue | Outcome |
| --- | --- |
| 1323 | Resolved. `--check` walks the ledger, and a row the extraction does not back needs `retired` or `unextracted`. It found 97 such rows on main. |
| 1042 | Resolved. The nine rows were re-verdicted against rclrs 0.7.0, the verdict-versus-bucket contradiction is gated, and `--refresh` names the rows it may invalidate. |
| 1302 | Resolved. The refusal names the `clock` argument, and its five rows agree. |
| 0783 | Resolved. `TransportError` got a `Display`, and the `NodeError` row is corrected. |
| 0784 | Progress. The six plumbing exports are `#[doc(hidden)]`. Deleting zero-consumer types, choosing the facade's lead `Node` name and exporting `StandaloneNodeError` are maintainer decisions. |
| 1335 | Re-scoped. The C++ half is done; what remains is the nros-c entity storage shape, a maintainer decision. |
| 1303 | Open. It needs a maintainer decision on the link model of `failed_create_aborts.cpp` before the runtime refusals can be routed through `nros_log` and proven to reach a freestanding sink. |

The RFC-0089 and RFC-0096 status lines were checked and both read "Stable".

## Not in this phase, deliberately

- **phase-417 W5.f — the C service type taken from the contract.** It is a
  C-API and codegen change, not drop-in work, and it belongs with the C-side
  work.
- **phase-417 W2.a / issue 0793 — one C parameter store.** That is phase-426's
  territory, which is still active.

## Acceptance

A ported rclcpp node, meaning the upstream tutorial publisher and subscriber
plus the `topic-state-monitor` and the W3 fixture:

- builds against nano-ros on posix, Zephyr and FreeRTOS;
- needs no edit to its class body, and needs exactly the `main` and
  `CMakeLists.txt` edits RFC-0096 D5 lists;
- uses no file under `cmake/compat/`, because that directory no longer exists.

The api-parity ledger has zero `gap` rows and zero deprecated aliases.
