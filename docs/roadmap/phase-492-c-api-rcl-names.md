# Phase 492 — The C API drops `nros_`: upstream names, `rclc_` for the rest

**Status (2026-10-12). In progress — W1, W2, W3, W5 (callers of the renamed names) and W6 landed; W4, W7, W8 open.** Implements the
[RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
amendment "`nros_` is phased out of the C API; ours-only names take `rclc_`"
(2026-10-12). The C counterpart of the C++ `nros::` → `rclcpp::` flip
(phase-427/428). Runs beside [phase-491](archived/phase-491-c-parameter-server.md),
whose parameter surface already follows the rule.

## Why

A ported rclc file should compile unchanged, and a nano-ros C user should
learn one vocabulary. Today the C user API is mostly `nros_`: 468 ours-only
items, 151 with an upstream counterpart, of which 77 already carry upstream's
name natively and at least six also keep an `nros_` twin
(`scripts/api-parity.py --lang c`, rclc + rcl humble, at `d457eed61e`).

## Work items

**W1 — The rename map and its gates.**
Generate the map from the parity report and the ledger, one row per public C
name in the `nros-c` headers: upstream spelling (ADOPT / ADOPT-BOUNDED), a
distinct `rclc_` spelling (a `differs` row that does not conform), or
`rclc_<name>` (ours-only). Commit it as a reviewed table. Add a gate that
counts `nros_` names in the public C headers, as a ratchet that ends at zero,
and extend the parity tripwire to ours-only header names under
`include/rclc/`.
*Acceptance:* every one of the 619 items has exactly one target name; no target
collides with a name in `docs/reference/api-surface/rclc.json` unless the row is
an adoption; the 18 `differs` rows each carry a conform-or-rename decision.

**W2 — Upstream header layout.**
Declare the surface under `include/{rcl,rcl_action,rcl_lifecycle,rclc,rclc_lifecycle,rclc_parameter}/`,
using upstream's file names where upstream has the file. Ours-only families get
`include/rclc/<family>.h`. Move the return-code mapping and the documented
refusals out of `rcl_compat.h` into `<rcl/types.h>` and the family headers.
Every umbrella keeps the `#error` against upstream's real `rcl` headers.
*Acceptance:* `packages/api/nros-c/tests/compile` builds a TU that includes only
upstream paths; a TU that includes both ours and a stub of upstream's
`<rcl/types.h>` fails to compile with the named message.

**W3 — Rename the symbols, types and constants.**
In `nros-c`: `#[no_mangle]` names, the cbindgen configuration, opaque-size
constants, `nros_ret_t`/`NROS_RET_*` → `rcl_ret_t`/`RCL_RET_*` (our values),
`NROS_PARAMETER_*` and other user-visible constants. The `nros_` twins of
already-adopted names are deleted. Build knobs (`NROS_MAX_*`, `CONFIG_NROS_*`)
do not change.
*Acceptance:* the W1 ratchet reads zero for `packages/api/nros-c/include`;
`check-ffi-struct-mirrors`, `check-abi-bindings` and the opaque-size asserts are
green.

**W4 — C components stop calling `nros_cpp_*`.**
A C component gets an `rcl_node_t *` and an `rclc_executor_t *` from its
context and uses the C API, so `nros_cpp_*` becomes C++-private. The codegen
that emits C entries (`tests_c.rs`, `builder/west_app.rs`, the entry emitters
calling `nros_app_main` and `nros_boot_config_*`) emits the new names.
*Acceptance:* `git grep 'nros_cpp_' -- '*.c'` is empty outside `nros-cpp`'s own
tests; `c_param_talker_pkg` and the other C component packages build and pass
their cells.

**W5 — Migrate every caller** (after W7 and W8).
Examples (about 160 files), `packages/testing` (about 70), `packages/cli` (about
80), the book, `docs/reference/c-api-cmake.md`, the C getting-started page
(which states that the drop-in is one-directional), and CLAUDE.md's Naming
line ("nros — code shorthand (crates, Rust/C idents)" no longer covers C API
identifiers). No forwarders.
*Acceptance:* `just ci gate` green; `just ci matrix` green (the change touches
codegen); `git grep -E '\bnros_[a-z_0-9]+\(' -- '*.c'` finds only seam
implementations.

**W6 — Ledger.**
Re-key the C rows of `docs/reference/api-parity-ledger/*.json` to the new
names. Every ours-only row keeps `verdict: extension` and says the name is ours
inside upstream's prefix.
*Acceptance:* `just check api-parity` green with `--require-disposition`.

**W7 — Messages in rosidl's names and shape.**
The C pack of `rosidl-codegen` emits RFC-0089's "Messages take rosidl's C names
AND its member shape": rosidl struct, header and constant names;
`rosidl_runtime_c__String` and `__Sequence` members; `__Storage` +
`__init_in`; no allocating rosidl functions; the RFC-0033 `heap` arm removed
from the C emitter. The runtime types (`rosidl_runtime_c__String`, the primitive
sequences) ship once in `nros-c` under `include/rosidl_runtime_c/`. Lands
before W5 so every example moves once.
*Acceptance:* a TU written against micro-ROS's manual-buffer idiom for
`std_msgs/String` and `sensor_msgs/LaserScan` compiles unchanged; a TU calling
`std_msgs__msg__String__init` fails to compile with the message naming
`__init_in`; deserialising a string longer than its `capacity` returns an error
and writes nothing past it; a counting allocator sees zero allocations across
`__init_in`, serialise and deserialise; the RFC-0033 cross-language capacity
tests still pass for Rust and C++.

**W8 — Type support through the descriptor.**
RFC-0089 "`ROSIDL_GET_*_TYPE_SUPPORT` adopted as a pass-through": the
descriptor gains `serialize` / `deserialize`; the C pack emits
`<pkg>__<sub>__<Name>__nros_type`; `<rosidl_runtime_c/*_type_support_struct.h>`
define the three macros; publishers and subscriptions keep the descriptor;
`rcl_publish` / `rcl_take` exist and serialise through it into the carved
transmit buffer; `rclc_executor_add_subscription_with_context` loses its extra
argument; the typed helpers retire. Lands with W7.
*Acceptance:* the micro-ROS-style talker and listener in the RFC compile and
exchange messages with a ROS 2 peer unchanged; each refused line in the RFC is a
must-not-compile verdict row (`*-verdict`, issue 1620); a counting allocator
sees zero allocations across publish and take.

## Out of scope

* The port seams (RMW, platform, board C ABIs), CMake function and target
  names, and build knobs (RFC-0089 amendment, "What does NOT move").
* The C++ API, already on `rclcpp::`.

## Progress

### 2026-10-12 — W1, W3, W5, W6: the rename

- **W1.** `scripts/gen-c-rename-map.py` -> `docs/reference/c-api-rename-map.tsv`.
  It has one row per public `nros_`/`NROS_` name, classified as:
  - adopt (43), alias (10), ours (599), ret (18);
  - kept: seam (60), hook/entry (46), build (107).

  `check-c-api-nros-names` (fast line) holds every public name to the map. The
  count left to rename is a ratchet, now at **0**. The map is a RECORD of the
  rename: regenerating it on the renamed tree yields only the kept names.
- **W3/W5/W6.** 670 names renamed across Rust, headers, examples, codegen,
  tests, scripts, book, `docs/reference` and the ledger, with no forwarders.
  Decision records keep their wording. `NROS_CODEGEN_VERSION`/`_MIN` -> 13.
  Seven gates whose patterns named the old spellings now read the new ones.
- Measured deviations from the plan:
  - the five lifecycle `differs` verbs became
    `rclc_lifecycle_state_machine_*` (their rclc spelling is upstream's with a
    different contract);
  - the entry and boot conventions (`nros_app_main`, `NROS_APP_*`,
    `nros_app_config_t`, boot config, tier task memory) KEEP `nros_`, as link
    contracts with board startup code, like the port seams;
  - the C component API's names (`nros_c_*`) became `rclc_c_*` mechanically
    and are reworked by W4.

### 2026-10-12 — W2: upstream include paths

- 51 headers at upstream's paths (`<rcl/*.h>`, `<rclc/*.h>`,
  `<rcl_action/*.h>`, `<rcl_lifecycle/*.h>`, `<rclc_lifecycle/rclc_lifecycle.h>`),
  taken from the recorded rclc+rcl surface. Each brings in the whole C API
  and `<nros/rcl_compat.h>`. They use their own include guards, so the refusal
  beside real rcl headers still fires. Probe:
  `packages/api/nros-c/tests/compile/upstream_include_paths.c` in `just check c`.
- Deviation: `<nros/*.h>` is NOT retired. Those files are where the
  declarations, the entry hooks and the build facts live; the upstream paths
  are the public entry points over them, and one umbrella keeps any
  declaration from having two spellings. Moving the examples' own includes to
  the upstream paths is cosmetic and is left for W5's tail.
