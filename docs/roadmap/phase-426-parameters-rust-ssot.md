# Phase 426 — parameters get a Rust SSoT, and `ros2 param list` works

**Status (2026-09-12). W1–W6 landed, and the two items the 2026-09-11 re-audit
found PARTIAL — W4 and W5 — are now CLOSED.** The re-audit below is kept in
full: it is the record of what each acceptance actually measured, and its W4 and
W5 bullets are what this wave answered. Each carries a `CLOSED 2026-09-12` note
naming what landed. The line before the re-audit read "per-item acceptance not
re-audited here", which is what let two unmet acceptances sit for four months.
Commits for the original waves:
- W1: 4 commits.
- W2: `30f5e9f941`.
- W3: 6 commits.
- W4: `3bf30405f1` deleted both C++ stores, `98f0f5de7e` taught the parameter FFI which node is asking; the second half is 2026-09-12 (below).
- W5: `1505290ecd` (the C surface); the acceptance is 2026-09-12 (below).
- W6: `3e3f1ef2db`.

### The re-audit (2026-09-11) — what each acceptance actually measures

Done because the status line's own caveat ("per-item acceptance not re-audited
here") is a claim about the AUDIT, not about the work, and archiving turns on
the difference.

* **W1 — MET.** `NodeKey` is a per-entry node identity
  (`nros-params/src/types.rs:34`); `declare` / `get` / `set` / `apply` all take
  it (`nros-params/src/server.rs:392`, `:524`, `:547`, `:504`). The acceptance's
  exact case is a test: `two_nodes_hold_independent_values_for_one_name`
  (`server.rs:950`, 10 vs 20, `total_len() == 2` at `:975`), and the pre-change
  collision is asserted rather than described (`server.rs:957`–`960`).
* **W2 — MET.** `Executor::set_parameter` (`executor/spin.rs:9307`), node-named
  form `:9316`, through `ParameterServer::apply` at `:9338`. Atomic all-or-none
  with the SECOND parameter failing, as written:
  `parameter_services.rs:3003`, good value first (`:3009`), typo second
  (`:3013`), the good half asserted not applied (`:3023`–`3027`). The
  `ros2 param set` half is on the wire in W6's cell
  (`params_per_node_interop.rs:280`–`292`).
* **W3 — MET, including the part written to force a BUILD error.** Six services
  register per node FQN (`executor/spin.rs:8334`, reconciled `:8388`,
  enumerated `:8667`); both FQNs appear to `ros2 param list`
  (`params_per_node_interop.rs:224`, `:229`). The ceiling is a build-time panic
  naming the knob — `check_queryable_override`
  (`nros-zpico-build/src/runner.rs:272`, message at `:286`) — and the
  acceptance's own three-node case is a test:
  `a_three_node_image_on_the_default_table_is_refused_at_build_time`
  (`runner.rs:460`, `#[should_panic(expected = "CONFIG_NROS_MAX_QUERYABLES")]`).
* **W4 — PARTIAL.** The MEMBERS are gone, which was the SSoT defect: no inline
  `ParameterServer` on `rclcpp::Node` (the facade forwards to the executor
  store, `nros-cpp/include/nros/node.hpp:791`–`802`, stated at `:70`–`74`),
  `ComponentNode`'s facade went with the type (phase-427 W4), and
  `NROS_RCLCPP_MAX_PARAMS` is gone with its history documented (`node.hpp:113`–
  `124`). What is NOT gone is the standalone caller-owned
  `nros::ParameterServer<Cap>`, retained on purpose (`node.hpp:73`–`76`) and
  still reading its own `server_` rather than the table
  (`nros-cpp/include/nros/parameter.hpp:385`) — and the shipped C++ example is
  still written against it (`examples/native/cpp/parameters/src/main.cpp:20`),
  so THAT example's parameters remain invisible to `ros2 param get`. "Delete
  both C++ stores" is met for the members and not for the class. Whether the
  class should stay is a decision, not an oversight; it needs stating either way
  before this item reads DONE.

  **CLOSED 2026-09-12.** The decision is stated and it is DELETE:
  `nros::ParameterServer<Cap>` is ABSENT, the three shipped examples that used
  the caller-storage store are on `rclcpp::Node` / `nros_executor_*_param_*`,
  `nros::Seq<T, N>` moved onto the node API so the freestanding array
  capability moved rather than went, the node's parameter methods left
  `#ifdef NROS_CPP_NODE_HOSTED`, and `check-example-parameter-stores` keeps the
  class of defect out of `examples/`. Full argument and measurements in the W4
  work item below.
* **W5 — PARTIAL, and the missing half is the acceptance itself.** The C surface
  does point at the same table and can name a node — fourteen `_on` spellings
  (`nros-c/include/nros/parameter.h:491`–`548`) onto
  `Executor::declare_parameter_on` / `get_parameter_on` / `set_parameter_on`
  (`nros-c/src/parameter.rs:936`, `:984`, `:1036`). But **no mixed C/C++
  workspace fixture declares from one language and reads from the other**: the
  features workspace ships `c_params` and `cpp_params` as separate
  single-language images (`examples/fixtures.toml:3709` and `:3775`;
  `examples/workspaces/features/src/demo_bringup/launch/{c,cpp}_params.launch.xml`,
  with no `mixed_params` sibling), `examples/workspaces/mixed/` declares no
  parameters at all (`.../mixed/src/demo_bringup/system.toml:17` is a services
  demo), and the nearest test is one-language-per-entry
  (`packages/testing/nros-tests/tests/cpp_c_param_live_read_e2e.rs:34`). The
  legacy divergent C store is also still exported
  (`nros-c/src/parameter.rs:136`) and still what the shipped C example uses
  (`examples/native/c/parameters/src/main.c:34`), so "C and C++ cannot disagree"
  holds for the `nros_executor_*` path and is untested across a single image.

  **CLOSED 2026-09-12.** `mixed_param_talker_pkg` is the missing image: one
  node, a C++ component and a C translation unit, C++ declaring
  `publish_period_ms` and reading `scale` while C declares `scale` and reads
  `publish_period_ms`. Row `workspace-features-mixed-params`, test
  `cpp_c_param_live_read_e2e::mixed_c_cpp_param_declare_and_read_cross_languages`.
  The legacy divergent C store is still exported — deliberately, see the W4
  item — but no example uses it any more. Measurements in the W5 work item
  below.
* **W6 — MET.** Cell `native-params-per-node-rust-zenoh`
  (`nros-tests/src/interop.rs:457`) runs `list` / `get` / `set` against a
  running two-node image and asserts the FQNs
  (`params_per_node_interop.rs:198`, whole-graph `:224`/`:229`, per-node
  `:241`/`:249`, `get` `:266`/`:271`, `set` isolation `:281`–`292`), with the
  coordinate tripwire at `:378` and the fixture at
  `examples/fixtures.toml:1485`. The literal "fails on a tree with W1 reverted"
  is not recorded as a run; the assertions are structurally the ones a flat
  table fails.
* **Both items W4 "inherits and nobody owns" are now OWNED and present.**
  `has_parameter` has its FFI (`nros-cpp/src/params_shim.rs:793`, header
  `nros_cpp_ffi.h:3395`, declared `node.hpp:816` with the `std::string` overload
  at `:833`), resolving against the executor store (`params_shim.rs:806`); and
  `declare_parameter<std::vector<T>>` has its forwarding target
  (`node_parameters.hpp:246`, read side `:260`, type map `:319`), with the
  previously-unowned caller named at `:207`–`218` and `std::vector<bool>`
  deliberately still absent (`:217`).

**The lesson, since it is the third time in this repo:** "landed" is a claim
about commits and "accepted" is a claim about the tree, and writing the first
where the second belongs makes the gap invisible. W4's acceptance named
`rclcpp::Node::declare_parameter` and `check-cpp-capability-layout`, and both
stayed true while `examples/native/cpp/parameters` went on demonstrating a
different store. A phase line that says "acceptance not re-audited" is a phase
line that says nobody checked.

Run `git log --grep='phase-426 W'` for the full list. There is one known hole in the
promise this phase makes. On Cyclone, the parameter services never start, so
`ros2 param list` sees the parameters on zenoh only. That is issue 1268, owned by
[phase-444](phase-444-rmw-fix-up.md) W6. The remaining user-API parameter gaps (27
ledger rows: descriptors, callbacks, undeclare) are listed in phase-444 § "The ROS 2
gap list".

Implements RFC-0089 §"Parameters:
feature-complete, Rust-side SSoT" and RFC-0019/0020's rule that behaviour lives
in Rust and the C/C++ APIs are thin wrappers. Closes the C++ half of issue 0793.

## Why

Three stores exist for one concept.

| store | where | who can see it |
| --- | --- | --- |
| `nros_params::ParameterTable` | Rust, on the **executor** | the six ROS 2 parameter services |
| `ParameterServer<NROS_RCLCPP_MAX_PARAMS>` | C++, inline on `rclcpp::Node` | that node's own getters, nothing else |
| a second facade | C++, on `ComponentNode` (`nros.hpp:394-405` flags the duplication against itself) | that object's own getters |

A parameter declared through either C++ facade is invisible to `ros2 param
get`, because the services read the Rust table. That is not a missing feature —
it is a second implementation of one, which is exactly what RFC-0019/0020
forbids.

**The service list is already complete**, and this is worth stating so the work
is not mis-scoped: `register_parameter_services` (`executor/spin.rs:7182`)
registers `GetParameters`, `SetParameters`, `SetParametersAtomically`,
`ListParameters`, `DescribeParameters` and `GetParameterTypes` — the full
upstream set.

**The defect is KEYING, not coverage.** Those six are registered under ONE FQN,
built from the *executor's* `node_name` and `namespace`, and the table is
executor-global. But an image composes several nodes onto one executor — that is
the whole model (RFC-0089 §"The rclcpp node model"). So:

* `ros2 param list` enumerates the executor's node, not the image's nodes;
* two nodes declaring the same parameter name collide in one flat table;
* nothing distinguishes `/talker`'s `rate` from `/listener`'s `rate`.

Upstream's model is one table and one set of six services **per node**.

## Work items

Ordered, and the order is load-bearing: **W4 must be last**, because deleting a
store before its replacement exists is how a capability disappears quietly.

* **W1 [rust] — key the table by node.** `ParameterTable` gains a node
  identity per entry; declare/get/set take it. The executor keeps one table
  (fixed arena, no per-node allocation), so this is a key widening, not a
  container per node.
  *Acceptance:* two nodes on one executor declare the same name with different
  values and both read back correctly; a unit test asserts the pre-change
  behaviour would have collided.

* **W2 [rust] — the missing writer.** There is no `Executor::set_parameter`
  and no `nros_cpp_set_param` (`grep -c` is 0). `SetParameters` and
  `SetParametersAtomically` are registered services with no store-side writer
  reachable from the wrapper path.
  *Acceptance:* `ros2 param set` changes a value an in-image node then reads;
  the atomic variant either applies all or none, asserted by a test that makes
  the second parameter fail.

* **W3 [rust+rmw] — six services PER NODE, and the sizing decision that comes
  with it.** Register the set under each node's FQN so `ros2 param list`
  enumerates the image's nodes.
  **This is not free and must not be discovered at runtime:** a service server
  IS a zenoh queryable. CLAUDE.md already records `[param_services]` claiming
  six slots against `ZPICO_MAX_QUERYABLES` (8 embedded by default); per-node
  registration makes that six times the node count. W3 therefore includes
  choosing the default, making the ceiling a build-time error rather than a
  runtime `-80`, and documenting the knob.
  *Acceptance:* a two-node image exposes both FQNs to `ros2 param list`; a
  three-node image on the default `ZPICO_MAX_QUERYABLES` fails the BUILD with a
  message naming the knob, not the boot.

* **W4 [cpp] — delete both C++ stores.** `rclcpp::Node`'s inline
  `ParameterServer` member and `ComponentNode`'s facade become forwarders to
  the FFI, then the members go. Only after W1–W3.
  *Acceptance:* a parameter declared through `rclcpp::Node::declare_parameter`
  is visible to `ros2 param get`; `check-cpp-capability-layout` still passes
  (the members leaving must not reintroduce a probe-dependent layout);
  `NROS_RCLCPP_MAX_PARAMS` is gone or documented as an arena bound, not a
  second store.

  **DONE — first half 2026-09-07, second half 2026-09-12.** The first half
  deleted the two node-owned stores and met the acceptance as written. What it
  left standing was a THIRD one: `nros::ParameterServer<Cap>`
  (`packages/api/nros-cpp/include/nros/parameter.hpp`), the C++ wrapper over
  the caller-storage C `nros_parameter_server_t` — and
  `examples/native/cpp/parameters`, the shipped example a user copies out,
  was nothing BUT that class. Its parameters are invisible to `ros2 param
  get`, which is the defect this phase exists to remove, live in the file
  that answers "how do I use parameters in C++".

  **`nros::ParameterServer<Cap>` is ABSENT now — deleted.** The disposition
  is RFC-0089's fourth, and it is available here only because upstream never
  had the name (the alias rule binds where rclcpp HAS a name and we lack it).
  Three measurements decided it: (1) after the first half of W4, `grep -rn
  "ParameterServer<" packages examples book docs` found the class
  CONSTRUCTED in exactly one place in the tree — the example that documented
  it, so the "caller-owned store with no executor" caller does not exist;
  (2) it was the defect, shipped, and an example is copied out (RFC-0026);
  (3) it is wrapper-side BEHAVIOUR — a ~600-line header with a name table
  and a bump-allocated element pool — which RFC-0019/0020 and RFC-0089
  §"Parameters" forbid.

  **Two things moved with it, because deleting a store before its
  replacement exists is how a capability disappears quietly.**
  - `nros::Seq<T, N>` SURVIVES as a value and is now the node API's array
    form: `node.declare_parameter(name, Seq<double, 8>{…})` /
    `get_parameter` / `set_parameter` route through
    `nros_cpp_node_{declare,get,set}_param_{double,integer,bool}_array`. It
    was the only way a `-nostdinc++` node could express an array parameter
    (`std::vector<T>` reaches that FFI only under `NROS_CPP_STD`), and it is
    now strictly MORE than the deleted class had: a sequence parameter is
    visible to `ros2 param get` for the first time. The three
    `nros_cpp_node_set_param_*_array` entry points are new — the declare/get
    pair had landed with no setter, so an array parameter was writable from
    the wire and from nowhere in C++.
  - The node's parameter METHODS left `#ifdef NROS_CPP_NODE_HOSTED`. They
    were inside it, so a freestanding C++ node had no parameter method at
    all and `ParameterServer<Cap>` was its only parameter surface; deleting
    the class with the gate in place would have removed the capability from
    freestanding C++ rather than moving it. `node.hpp` already said the
    member leaving was "the last thing that made `NROS_CPP_NODE_HOSTED`
    decide whether a node HAS parameters rather than whether it can spell
    them" — the declarations had simply not followed. Only the
    `std::string`-KEYED overloads keep the gate, which is a gate on METHODS
    and what `check-cpp-capability-layout` permits.

  **The C example was the same defect one language over**, and
  `examples/native/c/custom-transport-loopback` built a
  caller-storage store it then never touched. All three are on
  `nros_executor_*_param_*` now.

  *Measured 2026-09-12* (two shipped examples, one `rmw_zenohd`, ROS 2 Humble
  `rmw_zenoh_cpp`; `--no-daemon`, because a `ros2` daemon caches the graph
  across domains and answers "Node not found" for a node that is right
  there):

  ```
  ### ros2 node list
  /c_parameters
  /cpp_parameters
  ### ros2 param list /cpp_parameters
    ctrl_period / frame_id / max_iters / mpc_weights / verbose
  ### ros2 param get /cpp_parameters mpc_weights
  Double values are: array('d', [4.0, 5.0, 6.0, 7.0])
  ### ros2 param set /cpp_parameters ctrl_period 0.25
  Set parameter successful
  ### ros2 param list /c_parameters
    publish_rate_hz / scale_factor / topic_name / verbose
  ### ros2 param get /c_parameters scale_factor
  Double value is: 1.0
  ### ros2 param set /c_parameters scale_factor 2.5
  Set parameter successful
  ```

  `mpc_weights` is the `Seq<double, 8>`: the array the deleted class could
  not have shown to `ros2 param get` at all.

  **The gate, because a gate would have caught this and there was none.**
  `check-example-parameter-stores` (fast lane) refuses any reference from
  `examples/` to the caller-storage family. It is not a ban on the API —
  `nros-c` implements it and its compile test pins the entry points — it is a
  rule about what we SHOW. It self-tests its pattern against the three files
  that carried the defect, so it cannot pass vacuously once they are fixed.

* **W5 [c] — the C surface follows.** `nros_parameter_*` keeps its shape and
  points at the same table, so C and C++ cannot disagree about what a node's
  parameters are.
  *Acceptance:* a mixed C/C++ workspace fixture declares from one language and
  reads from the other.

  **DONE — surface 2026-09-07 (`1505290ecd`), ACCEPTANCE 2026-09-12.** The
  surface landed and the acceptance did not exist. Two corrections to the
  item as written, both worth recording because the wording misleads:
  - It is NOT `nros_parameter_*` that points at the table. That family is
    still the caller-storage store and still node-local. What W5 added is
    the `_on` half of `nros_executor_*_param_*` (`parameter.h`, the
    "Per-node spellings (phase-426 W5)" block), which names a node bound to
    the executor. `nros_parameter_*`'s shape is unchanged because nothing
    changed about it.
  - `workspace-features-{c,cpp}-params` are two SINGLE-LANGUAGE images.
    Each reads a launch-seeded `publish_period_ms` through
    `nros_cpp_get_param_integer` and publishes it; neither crosses, so "C
    and C++ cannot disagree" had no cell behind it.
    `examples/workspaces/mixed` declares no parameters at all.
    `packages/api/nros-c/tests/run/executor_param_node_keying.c` (also W5)
    proves the C `_on` family keys by node, which is a different claim.

  **What the acceptance is.** `mixed_param_talker_pkg` in
  `examples/workspaces/features` — ONE package, ONE node, two translation
  units and two languages. `src/ParamTalker.cpp` is the component;
  `src/param_probe.c` is C, handed the same `nros_cpp_node_t*`. Each
  language declares one parameter and reads the other's:
  C++ declares `publish_period_ms` (adopting the launch `<param>` seed of
  250) and reads `scale`; C declares `scale` (3.0) and reads
  `publish_period_ms`, live, on every tick. The image publishes the PRODUCT
  on `/chatter`, so the number on the wire is the conjunction of both
  crossings and is a value neither language writes down. A failed crossing
  also fails the BOOT: `configure` checks both directions and refuses, so a
  broken store shows up as a diagnostic rather than as a wrong integer.

  *Coordinate:* `linux / cpp / zenoh / workspace`, fixture row
  `workspace-features-mixed-params`, image `native_mixed_params`, launch
  `mixed_params.launch.xml`. Same cell as `workspace-features-cpp-params`,
  so it needs no new `matrix::CELLS` entry. Test:
  `tests/cpp_c_param_live_read_e2e.rs`'s third arm,
  `mixed_c_cpp_param_declare_and_read_cross_languages`.

  *Measured 2026-09-12* (build: `scripts/build/workspace-fixtures-build.sh
  linux cpp --id workspace-features-mixed-params` →
  `built: examples/workspaces/features/build/posix-zenoh-native/cmake/native_mixed_params_entry`):

  ```
  ### entry stdout
  Crossed: cpp declared publish_period_ms=250, c read 250; c declared scale=3.0, cpp read 3.0
  Published: 750     (x many)

  ### ros2 node list                 -> /param_talker
  ### ros2 param list /param_talker  -> publish_period_ms, scale
  ### ros2 param get  /param_talker scale              -> Double value is: 3.0    (declared in C)
  ### ros2 param get  /param_talker publish_period_ms  -> Integer value is: 250   (declared in C++)
  ### ros2 topic echo /chatter --once                  -> data: 750
  ```

  ```
  PASS nros-tests::cpp_c_param_live_read_e2e mixed_c_cpp_param_declare_and_read_cross_languages
  ```

* **W6 [test] — the cell that would have caught this.** An interop cell that
  runs `ros2 param list` / `get` / `set` against a running nano-ros image and
  asserts the node FQNs. There is no such cell today, which is why three stores
  could coexist without a red.
  *Acceptance:* the cell fails on a tree with W1 reverted.

## Not in scope

* Parameter **callbacks** (`add_on_set_parameters_callback`) — a separate
  ledger question, and adding a callback path before the store is single would
  be a third implementation.
* Parameter **overrides from launch/`NodeOptions`** — RFC-0060 territory.
* Making `nros::ComponentNode` disappear; that is the node-type merge
  (RFC-0089 decision 2), which W4 touches but does not depend on.

## Risk

W3 is the one that can go wrong quietly. Six queryables per node against a
default of 8 means a two-node image is already over on embedded, and the
failure mode today is a runtime `-80` from zenoh-pico, not a build error. The
acceptance criterion is written to force the build-time answer, because a
runtime one is how issue 0460 read to the person who hit it.

## Conciliated with phase-427 (2026-09-07) — this phase gates the node merge

Full reasoning in RFC-0089 §"Conciliating phase-426 and phase-427". Three
changes to this document.

**This phase's "Not in scope" was right; phase-427's mirror image was not.**
Keying the table, adding the writer and registering per node are Rust and RMW
work that does not care how many C++ node types exist. But phase-427 W1 cannot
proceed until W4 here lands: the parameter store is not hosted-only, so the node
merge's `void* hosted_` does not move it, and one type means one store size.
Measured today — `::nros::Node` 192 B, `rclcpp::Node` 3 752 B,
`::nros::ComponentNode` 55 776 B, the difference being entirely the inline
`ParameterServer`. Taking either store is a defect; deleting both is W4.

**So W4 is the gate on another phase, not a tidy-up at the end of this one.**
Its ordering constraint is unchanged and still load-bearing — W4 last, after
W1–W3, because deleting a store before its replacement exists is how a
capability disappears quietly — but its priority is higher than "cleanup"
suggests.

**The memory consequence, which this document does not state.** Forwarding to
the Rust store is right on SSoT grounds and it is not free
(`executor/spin.rs:7895`): the table is a leaked heap allocation of **285,184
bytes at the default `MAX_PARAMETERS=32`**, and 2,281,472 at 256, because
`ParameterValue` is sized by its `StringArray` variant and every slot costs
~8.5 KiB whatever it holds.

| image | today | after W4 |
| --- | ---: | ---: |
| any node count, no parameters declared | 3.5–55 KB per node | **0** — the table is lazy |
| one node using parameters | 55 KB | **285 KB** |
| five nodes using parameters | 277 KB | 285 KB |

A large win for images that declare nothing, roughly neutral at five nodes, and
a REGRESSION for a small one-node image that uses them. Issue 0756 already
records 256 slots overrunning the Zephyr thread stack and hanging boot with no
output. The ~8.5 KiB per slot is phase-382's defect, not this phase's, but W4's
acceptance should state the number for the image it lands on rather than let a
Zephyr build discover it.

**Two things W4 inherits from the node-type work.** `has_parameter` has no FFI,
and `declare_parameter<std::vector<T>>` — the sequence form written for the
vendored ASI consumer — has no forwarding target at all and no work item that
owns it. Neither is mentioned in W2's "the missing writer", which names only
`set_parameter`.

**Line citation:** `register_parameter_services` is at `executor/spin.rs:7480`,
not `:7182`. The premise it supports is confirmed: `node_fqn` is built from
`self.node_name` / `self.namespace`, the executor's, and all six services
register under that one name.
