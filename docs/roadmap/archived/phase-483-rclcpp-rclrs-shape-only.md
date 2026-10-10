# Phase 483 — the user API has ROS 2's shape only: `rclcpp::` in C++, rclrs in Rust

**Status (2026-10-11). Done; archived.** W1 merged as #1835, W2 as #1842, W3 and W4 as #1881, W5 with this archive. Settles issue 0784 and finishes what
[RFC-0089](../../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
§"Settled: `nros::` is phased out entirely" decided for C++ and stated as the
end state for Rust ("`rclrs::` for Rust"). Follows
[phase-482](phase-482-rclcpp-drop-in-residue.md), which made a ported ROS 2
C++ node build and run with no compat layer but left the `nros::` vocabulary
standing beside `rclcpp::`.

## Decisions taken when this phase was opened (2026-10-09, maintainer)

1. **C++: `nros::` stops being a namespace a user writes.** Every user-facing
   C++ name is defined in `rclcpp::`, `rclcpp_action::` or
   `rclcpp_lifecycle::`. There is no `nros::` alias left behind, because
   RFC-0089 already settled that `rclcpp::` is the HOME and not an alias onto
   `nros::`, and because no release carries the old spellings (the same
   argument phase-482 W6 used to delete 32 deprecated aliases in one batch).
2. **RTOS extensions stay.** A name with no upstream counterpart that exists
   for an RTOS reason (caller-supplied static storage, polling entities,
   scheduling contexts, board run loops, fixed-capacity strings and
   sequences) keeps existing. Per RFC-0089 it lives in upstream's namespace
   with ledger `disposition: extension`, and the existing tripwire refuses it
   if upstream ever defines the same name. Where an extension would sit under
   an upstream NAME with a different contract, the `_in` rule applies and it
   takes a different name.
3. **Rust: the `nros` crate has rclrs's shape, and a user may rename it.**
   `use nros as rclrs;` (or `rclrs = { package = "nros", … }` in
   `Cargo.toml`) must make a ported rclrs program read as rclrs. The crate
   keeps its name.
4. **Rust: one node type.** A node in a workspace component and a node in a
   standalone program are the same type, `nros::Node`, with rclrs's methods.
   Today there are five node-shaped types (the `Node` trait, `DeclaredNode`,
   `NodeHandle`, `NodeCtx`, `StandaloneNode`), and which one a user holds
   depends on how the program is built. That split is what this phase removes.

## Where the tree stands (measured 2026-10-09, `origin/main` `2b81536214`)

**C++.** The node class is already unified: `rclcpp::Node` is both the
workspace component base and the standalone node (phase-427 merged
`nros::ComponentNode` into it). What is left is the namespace. 147 names are
declared in `nros::` across `packages/api/nros-cpp/include/nros/*.hpp`
(23,787 lines), and `nros::` appears 1,598 times in those headers, 923 times
in `nros-cpp/tests`, 377 in `examples/`, 233 in `book/` and about 130 in the
C++ the CLI emits (entry packs, scaffold, `rosidl-codegen` message headers).
Examples write `nros::create_node`, `nros::init`, `nros::spin_once`,
`nros::Timer`, `nros::ErrorCode`, `nros::GoalResponse`, the `Poll*` family and
the `*Storage` types.

**Rust.** Five node-shaped types, split by program shape:

| type | how a user gets it | used by |
| --- | --- | --- |
| `nros::Node` trait + `nros::node!` | implement it; `register(&mut NodeContext)` | 78 example files (every workspace component) |
| `DeclaredNode` | `NodeContext::create_node` inside `register` | the same 78 |
| `NodeHandle<'a>` | `Executor::create_node` | 17 example files (native RTIC/async/serial, px4) |
| `NodeCtx<'e,'s>` | `Executor::node_mut(id)` | 7 of those 17 |
| `StandaloneNode` | `StandaloneNode::new` | two bench programs |

`NodeHandle` and `NodeCtx` are siblings, not one wrapping the other, and
neither carries rclrs's whole method set: callbacks, timers and graph queries
are on `NodeCtx`, while `name`/`logger`/`create_client` are on `NodeHandle`.
RFC-0089's own sketch already says `Executor::create_node -> Node`.

What a single type must keep working, from the survey:

1. **Per-class static sizing.** `ComponentSlotStorage` is sized in
   const-generic position from a per-type const (`ENTITY_BOUNDS`).
2. **Per-package C-ABI exports** for install, dispatch strategy and the
   framework callback trampoline.
3. **Name-keyed dispatch for RTIC/Embassy** (RFC-0043 Q10). This is an RTOS
   extension and stays one.
4. **The metadata probe** runs a component's registration with no transport.
5. **Borrowing.** `NodeCtx` holds `&mut Executor`, so only one is live at a
   time. rclrs nodes are `Arc`, so several can be held at once.

## Work items

### W1 — C++: the user API moves out of `nros::`

- Every declaration in `namespace nros` moves to `rclcpp::` (or
  `rclcpp_action::` / `rclcpp_lifecycle::` where upstream keeps the
  concept). Internals stay in a `detail` namespace.
- Self-aliases disappear (`nros::Publisher<M> = rclcpp::Publisher<M>` and
  its siblings).
- Collisions are resolved by upstream's shape, not ours:
  - the QoS policy enums become `enum class` with upstream's enumerator
    names, which frees `rclcpp::KeepLast` / `KeepAll` for the upstream
    profile helpers;
  - the base `QoS` class merges with `rclcpp::QoS`;
  - `Result`-returning `init` / `shutdown` / `spin` overloads that would sit
    under upstream's names with a different contract take the `_in` suffix.
- Every in-tree consumer moves in the same change: examples, templates,
  `nros-cpp/tests`, the CLI's C++ emitters and their goldens, the
  `rosidl-codegen` message headers, the book, the gates and the API-parity
  extractor and ledger.

**Acceptance:** no `nros::` in any C++ a user writes or the CLI emits; a gate
keeps it so; `just check cpp` and `just ci gate` green; the Zephyr and
FreeRTOS port templates still run.

**Status 2026-10-10: done.**

- The namespace moved, and no alias was left. Collisions went upstream's way:
  - the QoS policy enums are `enum class`, with `LivelinessNone` now
    `LivelinessPolicy::SystemDefault`;
  - the two QoS classes are one class, with upstream's implicit
    `QoS(size_t)` constructor;
  - `init_in` / `shutdown_in` / `spin_in` carry the `Result`-returning forms
    whose names upstream uses with a `void` / `bool` contract;
  - the goal vocabulary lives in `rclcpp_action::`, and the lifecycle
    vocabulary in `rclcpp_lifecycle::`;
  - the deprecated `nros::LifecycleNode` forwarder and its warning probe are
    deleted.
- Generated C++ names the new spellings, so `NROS_CODEGEN_VERSION` and its
  minimum are both 10. The committed interface crates are restamped, and the
  `nros --codegen-version` smoke expectation follows.
- Gate `check-cpp-no-nros-namespace` (fast line). `check-codegen-version-surface`,
  `check-qos-profile-ssot`, `check-cpp-subscription-bound-supplied` and
  `check-cpp-capability-layout` now read `rclcpp::`.
- API-parity ledger:
  - rows `cpp:init_in`, `cpp:shutdown_in` and `cpp:spin_in` are added;
  - `cpp:ActionClient` and `cpp:ActionServer` are deleted, because they
    described aliases that no longer exist.
- `check-template-copy-out` builds only HOST images. The port template's
  leaves are cross-board, and the gate had tried to build the template root
  as a workspace (red on main since W3). Superseded by issue 1764's fix: the
  gate now builds each sub-project (the FreeRTOS leaf builds; the Zephyr one
  is a named skip, issue 1782).
- Measured:
  - `just check cpp` passes;
  - the CLI's 2288 tests pass;
  - 403/403 fast gates pass;
  - `test-unit` and `test-lane-contracts` pass;
  - `build-test-fixtures lane=native` passes;
  - the native C++ e2e tests pass.
- Pre-existing and unrelated, so not fixed here:
  - `check build`'s `workspace-features` fails on a duplicate-symbol link in
    `nros-board-threadx`'s lib test (issue 1772);
  - `dist-runtime-deps` needs `libatomic` for the local `arm-fvp` dist.

### W2 — Rust: one `Node` type, rclrs-shaped

- `nros::Node` becomes the node struct. It carries the union of today's
  `NodeHandle` and `NodeCtx` methods under rclrs's names: `name`,
  `namespace`, `fully_qualified_name`, `logger`, `domain_id`, `get_clock`,
  `create_publisher`, `create_subscription(topic, closure)`,
  `create_service`, `create_client`, `create_action_server` / `client`,
  `create_timer_repeating` / `oneshot`, `declare_parameter` and the graph
  queries.
- `Executor::create_node` returns it.
- The component trait is renamed (it is the analogue of
  `rclcpp_components`, not of a node). Its registration receives
  `&mut Node`, the same type a standalone program holds, so one body of
  node code works in both.
- The per-type const, the C-ABI exports and the probe's recording runtime
  stay behind the trait. The node itself does not know whether it is live or
  being recorded.
- `NodeHandle`, `NodeCtx`, `DeclaredNode` and `NodeContext` stop being
  public names.

**Design (2026-10-10).** The unifying move is to make the component register
against the SAME live executor a standalone program uses, rather than against
a declaration-recording runtime:

1. `NodeCtx` (the executor-borrowing handle that already carries rclrs's
   callback-taking `create_subscription` / `create_service`, timers, clock and
   the graph queries) becomes `nros::Node`. It gains what only `NodeHandle`
   has: `name`, `namespace`, `fully_qualified_name`, `logger`, `domain_id`,
   and the polled entity constructors, which are RTOS extensions and take
   `create_polling_*` names so they do not sit under rclrs's callback-taking
   `create_*` names with a different contract (the `_in` rule's reasoning,
   applied to Rust). `Executor::create_node` returns it.
2. The component trait is renamed `nros::Component` (it is the analogue of
   `rclcpp_components`, and `Node` is now the node). Its registration takes
   the executor and creates its node exactly as a standalone `main` does:
   `fn register(executor: &mut Executor) -> Result<(), NodeError>`. The
   launch identity, QoS overrides, parameters and remaps that
   `ExecutorSink` applies today are installed on the executor before
   `register` runs and consumed by the component's first `create_node`.
3. The metadata probe stops needing a recording runtime: it opens an executor
   on the metadata backend and runs `register` against it, which is how the C
   and C++ probes already work (`nros::census_hooks`, issue 1419). A Rust
   component therefore records through the same hooks a C++ one does.
4. Name-keyed dispatch (`ExecutableNode`, `on_callback`) stays as the
   RTIC/Embassy extension, reached through `Node` extension methods that bind
   a callback NAME instead of a closure. A component that is not
   framework-dispatched writes closures, as rclrs does.

**Status 2026-10-10: the node type is done; the component side moves in W3.**

- `NodeCtx` is now `nros::Node`, and `Executor::create_node` /
  `create_node_on` return it.
- It gained the constructors and accessors only `NodeHandle` had: `name`,
  `namespace`, `fully_qualified_name`, `domain_id`, `logger`, `id`, the
  client, action and `_sized` / `_raw` constructors. Each forwards through
  `Executor::with_node_try`.
- The polled subscription and service constructors are now
  `create_polling_*`.
- The TYPED QoS-override table stays a `NodeHandle` input, reached through
  `with_node_try`. Storing it on the node record grew every executor's node
  table by a slice, and a ThreadX image's stated executor backing then sat
  below the derived size. A node's overrides are the executor's code table,
  as before.
- The component trait is renamed `nros::Component`.
- `NodeHandle` leaves the facade and its prelude.
- The ledger re-keys `rust:Node` to the handle, now `adopt-bounded` with its
  borrow envelope, and adds the `extension` rows. The `NodeHandle` rows are
  retired.
- Still open, and the subject of W3: a component's `register` hands it
  `DeclaredNode`, not `Node`.

### W3 — Rust: migrate every consumer

The 78 component files, the 17 imperative files, `packages/testing`, the
`nros::node!` / `nros::main!` expansions, the CLI's Rust emitters and the
metadata probe harness all move to W2's `Node`. Name-keyed framework dispatch
stays as the RTIC/Embassy extension.

**Status 2026-10-10 (W3 core): a component's `register` gets the one `Node`.**

- `NodeContext::create_node` returns `nros::Node`, the type
  `Executor::create_node` returns, and `NodeContext` lost its runtime type
  parameter.
- `DeclaredNode` is gone. Its constructors are the `nros::DeclarativeNode`
  extension trait, implemented for `Node`. The four explicit-id forms that
  would have shadowed rclrs-named inherent methods are now `declare_*`.
- Registration goes through a frame installed on the executor for the
  lifetime of the `NodeContext`. `Executor::__set_component_frame` is an
  opaque pointer, because nros-node sits below the crate that defines the
  frame. The frame's sink is either the live `ExecutorSink` or the probe's
  `RecordSink`.
- The sink receives the executor per call instead of holding it, because the
  `Node` the component holds borrows the same executor.
- The metadata probe opens an executor on the `metadata` backend:
  - `record_node_metadata` takes the executor too;
  - the generated harness depends on `nros-rmw-metadata` and links the posix
    C port.
- The component API now requires `rmw-cffi`. The non-`rmw-cffi`
  `install_node_typed*` stubs are deleted. Every example component already
  enabled the feature.
- Zero-consumer types deleted (issue 0784): `NodeRuntimeAdapter`,
  `RuntimeNodeRecord`, `DeclaredNodeRuntime`, `NodeExecutorRuntime`,
  `MISSING_NODE_EXPORT_ERROR`.
- The declarative-registration unit tests moved from `nros/src/node.rs` to
  `nros-rmw-metadata/tests/component_registration.rs`, which can open an
  executor.
  - 19 moved and pass.
  - The three `NodeRuntimeAdapter` tests went with the type.
  - New: a duplicate node name is refused, and
    `a_component_and_a_program_share_one_node_type`.
- The live component tests (`component_runtime`, `component_dispatch`,
  `component_param`, `tier_filter`, `dispatch_strategy`) pass, 15/15.

**Still open in W3:** examples keep the name-dispatched `ExecutableNode`
bodies. Writing them as rclrs-style closures on `Node` is optional per
example and not required by the acceptance.

### W4 — Rust: the facade's surface

- `StandaloneNode` and its error leave `nros::`; the two bench programs
  import `nros_node` directly.
- Remove the zero-consumer types from issue 0784.
- Move the remaining RTOS extensions out of the prelude into
  `nros::embedded`, which is already the stated rule.
- A test compiles a ported rclrs program under `use nros as rclrs;`.

**Status 2026-10-10: done.**

- `StandaloneNode`, `NodeConfig`, `PublisherHandle` and `SubscriptionHandle`
  left `nros::` and its prelude. The two bench programs depend on
  `nros-node` directly. Their ledger rows are retired.
- The zero-consumer types were deleted in W3.
- `tests/rclrs_talker_port.rs` compiles the ported tutorial under
  `use nros as rclrs;`. Its first line is now upstream's `use rclrs::*;`, so
  the port differs from upstream on five lines instead of six.
- The prelude's remaining extensions are each argued in
  `check-prelude-tiers`'s allow-list. `Component` and `DeclarativeNode` were
  added to it, because a component cannot register without them.
- Issue 0784 is resolved and archived.

### W5 — book, RFCs and ledger

- RFC-0089 gains the Rust half of the decision; RFC-0043, RFC-0044, RFC-0022
  and RFC-0036 note where they are superseded.
- The book's C++ and Rust chapters use only the upstream spellings.
- The API-parity ledger's `node.json` / `exec.json` rows stop describing the
  two-model split.
- Issue 0784 is resolved and archived.

**Status 2026-10-11: done.**

- **RFCs.** RFC-0089 carries the Rust half of the decision (§"Settled: `nros::`
  is deleted from C++, and Rust takes rclrs's shape with ONE node type") and
  now names both gates. RFC-0022, RFC-0036, RFC-0043 and RFC-0044 carry their
  supersession notes (W4).
- **Docs.** `docs/guides/cpp-api.md` still taught the retired `nros::` API in
  35 code lines. The `cpp-no-nros-namespace` gate read only `book/`, so it was
  green over them. The guide now uses `rclcpp::` / `rclcpp_action::`, with
  `init_in` / `spin_in` where the contract differs. Three of its sections had
  drifted further than the namespace and were rewritten against the headers:
  the action server (it is callback-driven, `rclcpp_action::Server<A>`), the
  `NROS_CPP_STD` section (free functions that do not exist, replaced by the
  ported `make_shared` / `SharedPtr` shape), and spinning (`spin_in`). Its code
  was compile-checked against the headers with a stub message type. Smaller
  fixes landed in `codegen-type-mapping.md`, `c-api-cmake.md`,
  `platform-implementation-notes.md`, `platform-differences.md`, the
  `violation-cpp` README and the workspace slides (`impl Component for`).
- **Ledger.** 100 rows ended "Whether the two Rust APIs should both be public,
  and which one `nros::` should lead with, is issue 0784"; they now state the
  answer. 22 rows described `NodeCtx` in the present tense and now say
  `nros::Node`. Six `node.json` rows (`RegisteredNode`, `install_node_typed*`)
  carried a pasted copy of `NodeHandle`'s "four node-shaped things" text and
  now describe their own names. 64 rows complained that `nros::` exports
  machinery beside the user API, citing 0784. That half was never settled, so
  those rows now cite [issue 1789](../../issues/1789-nros-facade-exports-runtime-machinery.md), filed for it.
- **Two gates that had gone blind, found during the sweep:**
  - `check-component-entity-bounds` matched `impl … Node for`, so after W2
    renamed the trait it read 2 classes of ~104 and reported OK. It now
    matches `Component` and counts the post-W3 `declare_*` constructors.
    Self-tests cover both; the old regex fails 5 of them.
  - The two classes it still read were
    `packages/testing/nros-tests/bins/param-store-{nuttx-qemu-arm,threadx-riscv64}`,
    which still wrote `impl Node for` and no longer compiled. Both are ported.
    The NuttX row builds; the two sources are identical. The ThreadX row stops
    earlier, in the NetX Duo build (`nx_port.h: No such file or directory`),
    which this change does not touch.
- **Rust gate.** `check-rust-one-node-type` is new (below).

## Acceptance

- A C++ user writes no `nros::`.
- A Rust user writes `use nros as rclrs;` and gets rclrs's shape, with one
  `Node` type in every program.
- Both are enforced by gates, not by review.

**Met 2026-10-11.**

- **C++:** `check-cpp-no-nros-namespace` reads the C++ sources, the CLI's
  emitted C++, and every ```cpp block in `book/`, `docs/guides/`,
  `docs/reference/` and `examples/**/*.md`.
- **Rust:** `check-rust-one-node-type` refuses a `pub trait Node` in the
  facade and a re-export of any node-shaped name phase-483 retired. It flags
  eight retired-name exports in the pre-W2 `lib.rs`.
- **Rust compile check:** `tests/rclrs_talker_port.rs` compiles the ported
  rclrs talker under `use nros as rclrs;`, in `required-features-tests`.
