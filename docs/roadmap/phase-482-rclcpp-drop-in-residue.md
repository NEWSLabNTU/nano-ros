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

### W4 — `rclcpp_lifecycle::LifecycleNode`

- A `Node`-derived `rclcpp_lifecycle::LifecycleNode` with upstream's
  `(name[, options])` constructor, the transition callbacks
  (`on_configure` … `on_shutdown`) and `LifecyclePublisher`.
- `nros::LifecycleNode` is a deprecated forwarder.
- **Acceptance:**
  - A ported lifecycle node's class body compiles unchanged.
  - The ledger's `cpp:LifecycleNode*` rows read `adopt` or `adopt-bounded`.
  - A runtime probe drives configure, activate, deactivate and cleanup.

### W5 — `rclcpp::init(argc, argv)`: `-p` and `--params-file`

- `-r` has been honoured since 2026-10-05. The parameter half is the ledger's
  last `gap` (`rust:init_with_args`), and it is the same work in C++ and C.
  Overrides apply to the node's parameter store before the first `declare`.
- **Acceptance:** zero `gap` rows in the ledger, and a test that boots with
  `-p` and with `--params-file`.

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
