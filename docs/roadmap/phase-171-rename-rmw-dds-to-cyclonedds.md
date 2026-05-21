# Phase 171 — Rename `dds` RMW → `cyclonedds`; complete example matrix; no-alloc audit

**Goal.** Once Phase 169 retires dust-DDS, do the follow-on rename
+ matrix sweep: rename the RMW backend identifier `dds` →
`cyclonedds` everywhere it surfaces (Cargo features, CMake cache
vars, Kconfig values, example-tree directories, book docs),
fill the per-platform × per-language `cyclonedds` example matrix,
and audit the `nros-rmw-cyclonedds` wrapper for `no_std + no-alloc`
discipline. The wrapper stays C++ (Cyclone DDS's native language;
matches the RMW backend host-language policy frozen 2026-05-07).

**Status.** In progress. The Zephyr `native_sim` Cyclone DDS runtime
bring-up + Zephyr matrix-fill (formerly tracked as the standalone
**Phase 11W**, now absorbed here — see §171.0 below) has **landed**:
pub/sub works in all three languages (Rust/C/C++) and request/response
services work in Rust + C++ + C, with the supporting NSOS host patches,
backend `service_type_name` fix, and a stock-`rmw_cyclonedds_cpp`
double-slash topic-naming interop fix. The rust example migration off
`nros-rmw-dds` → `nros-rmw-cyclonedds-sys` has **landed** (§171.B.2,
commit `40efd9319`) for native / freertos / threadx-{linux,riscv64}.
**Decision 2026-05-21: keep Cyclone DDS support targeted at bare metal**
— the freertos / nuttx / baremetal / esp32 / riscv64-threadx rust cells
keep the `rmw-cyclonedds` feature *defined* (build gated on the Cyclone
RTOS port, §171.C.gate / `phase-175`), rather than being deleted as the
original 171.B draft proposed. Still open: the code-surface rename
(§171.A), the `dds/` example *directory* renames (§171.B.3), the
non-Zephyr matrix cells (§171.C.1/.3/.4/.5/.6), and the no-alloc audit
(§171.E). Zephyr cyclonedds actions remain open inside §171.0. Native
cpp+cpp CycloneDDS action `get_result` is fixed in `28e9e6502`.

**Priority.** P2 — paper-rename and matrix-fill on top of the
already-decided 169 retirement.

**Depends on.**
- **Phase 169** retire-dust-dds-consolidate-cyclonedds (must land
  first so the `dds` identifier unambiguously means Cyclone DDS).
- Phase 117 Cyclone DDS RMW bring-up (POSIX + Zephyr/cpp landed;
  stock-RMW interop slices 117.X.1–117.X.5 still open — those land
  on top of this rename, not blocked by it).
- Phase 131 examples-tree shape (canonical
  `examples/<plat>/<lang>/<rmw>/<example>/` layout).

> **Note.** Earlier-drafted as Phase 169 (file
> `phase-169-retire-dust-dds-rename-cyclonedds.md`); renumbered to
> 171 after a separate Phase 169 doc (`-consolidate-cyclonedds`)
> landed first and claimed the same number. Content scope narrowed
> to the rename + matrix + no-alloc audit since the retirement
> half is now Phase 169's job.

---

## Overview

Today the workspace ships two DDS backends:

- **`nros-rmw-dds`** — Rust crate wrapping a vendored `dust-dds`
  submodule (`third-party/dust-dds/`). `no_std + alloc`, embedded-
  friendly on paper, but Phase 117.2h surfaced a hard
  `Actor<DcpsStatusCondition>::poll` deadlock on Xtensa LX7 (tracked
  as Phase 166.F) that blocks ESP32-S3 close-out. Phase 71's
  `DdsRuntime` abstraction was supposed to make dust-dds platform-
  portable; in practice the actor mailbox shape clashes with
  non-reentrant `critical-section` impls and the maintenance cost
  has dominated every recent embedded port.

- **`nros-rmw-cyclonedds`** — C++ wrapper around Eclipse Cyclone
  DDS (`third-party/dds/cyclonedds/` pinned at tag `0.10.5` to
  match `ros-humble-cyclonedds`). Lands the canonical RTPS wire
  format used by the wider ROS 2 ecosystem; full wire-compat with
  stock `rmw_cyclonedds_cpp` is the explicit Phase 117 goal.
  Surfaces on POSIX (Phase 117) and on Zephyr `native_sim` across all
  three languages via the collapsed-shape `prj-cyclonedds.conf` overlay
  (§171.0, formerly Phase 11W), plus the one-board FVP reference
  `examples/zephyr/cpp/cyclonedds/talker-aemv8r/`.

Naming gap: every other surface (cargo features, cmake cache vars,
Kconfig values, example-tree directories, book docs) uses bare
`dds` to mean "dust-DDS". Once dust-DDS is gone, `dds` is a stale
identifier — `cyclonedds` is what the backend actually is.

This phase does two things, in order:

1. **Rename** `dds` → `cyclonedds` everywhere it surfaces in code,
   build glue, example dirs, and docs. Mostly mechanical, but
   touches enough surfaces that doing it as one atomic phase
   avoids half-renamed states.
2. **Complete the matrix**: fill every `<plat>/<lang>/cyclonedds/`
   cell that Cyclone DDS can actually build on, with `no_std +
   no-alloc` examples where the platform / language allow.

> **Note.** The dust-DDS retirement (delete crates + submodule)
> moved to Phase 169 as the prerequisite. By the time Phase 171
> starts, `nros-rmw-dds` and `third-party/dust-dds/` are already
> gone — this phase only deals with the lingering naming
> follow-ups and the example-matrix fill.

---

## Architecture

### Naming after this phase

| Concept             | Before               | After                |
|---------------------|----------------------|----------------------|
| Cargo feature       | `rmw-dds`            | `rmw-cyclonedds`     |
| Cargo crate         | `nros-rmw-dds`       | (deleted)            |
| Cargo crate         | `nros-rmw-dds-staticlib` | (deleted)        |
| Cargo crate         | `nros-rmw-cyclonedds-staticlib` (new) | `nros-rmw-cyclonedds-staticlib` |
| CMake cache var     | `-DNANO_ROS_RMW=dds` | `-DNANO_ROS_RMW=cyclonedds` |
| CMake macro         | `NROS_RMW_DDS=1`     | `NROS_RMW_CYCLONEDDS=1` |
| Kconfig value       | `CONFIG_NROS_RMW="dds"` | `CONFIG_NROS_RMW="cyclonedds"` |
| Example dir         | `examples/<plat>/<lang>/dds/` | `examples/<plat>/<lang>/cyclonedds/` |
| Example matrix col  | `dds`                | `cyclonedds`         |
| Backend host lang   | (dust-DDS = Rust)    | Cyclone DDS = C++ (frozen) |
| RMW enum variant    | `Rmw::Dds`           | `Rmw::CycloneDds`    |
| Submodule           | `third-party/dust-dds/` | (deleted)         |
| Submodule           | `third-party/dds/cyclonedds/` (kept) | `third-party/dds/cyclonedds/` |

### `no_std + no-alloc` policy for new examples

The remaining `cyclonedds` backend is C++ on a C++ DDS stack —
Cyclone DDS itself uses dynamic allocation internally and there's
no path to make THAT alloc-free. The policy applies to the
**example code and the nano-ros wrapper layer**, not to the C++ DDS
core:

- **Rust examples**: declare `#![no_std]`, no `extern crate alloc`,
  use `heapless::{Vec, String}` for any collections, static buffers
  for sample storage. The example app itself never touches `alloc`.
- **C examples**: stack-allocated message structs + fixed-size
  scratch buffers; no `malloc` in the app code (Cyclone DDS may
  allocate internally — that's transparent to the app).
- **C++ examples**: `nros-cpp` is freestanding C++14 with optional
  `std`; new cyclonedds examples target the freestanding mode
  (`NROS_CPP_STD=OFF`), use `nros::Vec`-style fixed-capacity
  containers, no `std::vector` / `std::string` in app code.
- **Wrapper code in `nros-rmw-cyclonedds`** (the package itself,
  not its tests): stays C++14 freestanding-compatible.
  `nros::Result` instead of `std::expected`, fixed-capacity
  containers, no `std::shared_ptr` / `std::unique_ptr` (use
  raw pointers + RAII guards from `nros-cpp`).

Platforms that don't yet support the chosen no-alloc shape (e.g. a
platform whose Cyclone DDS port still pulls in libc heap
unavoidably) document the constraint per-cell in
`examples/README.md` "Intentionally empty cells" — same shape as
Phase 118 / 131 used.

### Backend host-language policy update

`book/src/internals/rmw-backends.md` (RMW backend host-language
policy, frozen 2026-05-07) currently records:

> dust-dds=Rust, cyclonedds=C++, XRCE=Rust→C (115.K.2),
> zenoh-pico=Rust (deferred), uORB=Rust (won't-do).

After this phase:

> cyclonedds=C++, XRCE=Rust→C (115.K.2), zenoh-pico=Rust
> (deferred), uORB=Rust (won't-do). [dust-DDS retired Phase 169.]

---

## Work items

### 171.0 — Zephyr `native_sim` Cyclone DDS runtime (landed; absorbed Phase 11W)

Originally a standalone phase (`phase-11W-cyclonedds-zephyr-native_sim-runtime.md`,
now archived). It brought Cyclone DDS up to a working runtime on
`native_sim/native/64` and filled the Zephyr cyclonedds example cells.
Folded here because it *is* the Zephyr slice of §171.C.2 — but note the
**shape correction below** vs. this phase's original `<lang>/cyclonedds/`
assumption.

**Shape correction.** This phase's matrix (§171.C, the table, and the
"surfaces only in `examples/zephyr/cpp/cyclonedds/`" note) predates the
Phase 168 collapse. On Zephyr there is **no `<lang>/cyclonedds/`
directory** — the canonical shape is the collapsed
`examples/zephyr/<lang>/<example>/` dir with RMW selected at build time
via a `prj-cyclonedds.conf` overlay (`-DCONF_FILE="prj.conf;prj-cyclonedds.conf"`).
So §171.C.2 for Zephyr is "add the `prj-cyclonedds.conf` overlay + the
Cyclone C descriptor-gen CMake branch to each collapsed example", not
"create a `cyclonedds/` subtree". The native / threadx-linux cells
(§171.C.1/.3) keep the `<lang>/<rmw>/` shape since those trees weren't
collapsed.

**Landed (committed on `main`, phase-11W.12 commits):**
- [x] Compile + link + boot smoke for all 6 cases × 3 languages on
      `native_sim/native/64`.
- [x] **Pub/sub discovery — Rust + C + C++.** `test_zephyr_{rust,cpp,c}_cyclonedds_pubsub_e2e`
      (listener receives talker samples over SPDP multicast). Required:
      NSOS `getifaddrs` host trampoline + host-side `IPPROTO_IP`
      setsockopt forwarder (so `IP_ADD_MEMBERSHIP` reaches the host
      kernel) + distinct `--seed` per process (native_sim's deterministic
      test entropy otherwise yields identical Cyclone GUID prefixes →
      SPDP self-ignore). Patches wired idempotently into `just zephyr setup`.
- [x] **Services — Rust + C++.** `test_zephyr_{rust,cpp}_cyclonedds_service_e2e`
      (request/response roundtrip). Surfaced + fixed a backend bug:
      `service_type_name` concatenated `<base>_Request_` but the codegen
      emits `SERVICE_NAME` with a trailing `_`, giving a double-underscore
      lookup that missed the registered descriptor — now strips one
      trailing `_` (matches stock `rmw_cyclonedds_cpp`; no-op when absent).
- [x] **Stock-interop topic fix.** `topic_prefix::apply` no longer emits
      a double slash for leading-slash names (`rq//x` → `rq/x`), matching
      stock `rmw_cyclonedds_cpp`. Regression-checked against rust pub/sub
      + service E2E.
- [x] **nextest serialization.** `zephyr-native-cyclonedds` group
      (`max-threads=1`) — these tests bind the fixed SPDP multicast port
      and can't run concurrently (NSOS doesn't forward `SO_REUSEADDR`).
- [x] Overlay runtime parity for c/cpp/rust talker/listener + rust/cpp
      service examples (16 MiB malloc arena, NSOS offload forcing,
      NET_TCP, pthread pools) so the participant inits instead of
      crashing in picolibc libc-hooks.

**Open (Zephyr cyclonedds):**
- [x] **171.0.d — `nsos_adapt.c` duplicate-case build break (REGRESSION,
      blocks ALL cyclonedds-zephyr).** FIXED (Phase 177.1): the redundant
      `nsos-adapt-ipproto-ip-patch.sh` (11W.12) now skips when
      `case NSOS_MID_IPPROTO_IP` is already present (native-sim-ipproto-ip-patch.sh
      provides the complete case), so no second label is emitted. After
      the Phase 11W.10–.12 NSOS
      patch set, `zephyr/drivers/net/nsos_adapt.c` has TWO
      `case NSOS_MID_IPPROTO_IP:` labels in the `nsos_adapt_setsockopt`
      switch (lines ~788 and ~841): `nsos-adapt-ipproto-ip-patch.sh`
      (11W.12) adds a second IPPROTO_IP case to a switch that an earlier
      patch (mcjoin / udp-rcvbuf) already gave one → gcc `error:
      duplicate case value` → `zephyr.exe` link never reached. Breaks
      **every** cyclonedds-zephyr fixture (rust + c + cpp, all 6 cases =
      54 builds) on a clean `just zephyr setup`. Confirmed via
      `just build-all` 2026-05-21 (the sole `build-all` failure; the 7
      cargo platforms + zenoh/xrce-zephyr are green). Fix: make
      `nsos-adapt-ipproto-ip-patch.sh` MERGE its handling into the
      existing `case NSOS_MID_IPPROTO_IP:` body rather than emit a
      second label (idempotency guard already exists; the collision is
      cross-patch, not double-apply). Highest-priority 171.0 item — it
      gates everything else here.
- [x] **171.0.a — C service request delivery.** FIXED 2026-05-21.
      `test_zephyr_c_cyclonedds_service_e2e` now runs the C
      `service-server` + C `service-client` on `native_sim/native/64`
      and observes `Result: 5 + 3 = 8`. The C service examples now have
      the same CycloneDDS descriptor-generation CMake branch used by the
      Rust/C++ service examples, the C service overlays have matching
      native_sim runtime sizing/offload config, and Zephyr uses local
      CycloneDDS compatibility shims for `std::chrono::steady_clock` and
      `ddsrt_getprocessname`/`ddsrt_getpid` instead of the POSIX
      `/proc/self/cmdline` path. Also fixed the `just zephyr
      build-fixtures` env-array invocation so fixture prebuilds execute
      the `west build` argv instead of trying to run an env assignment as
      a command.
      - **Root cause / fix (2026-05-21).** Service QoS is RELIABLE +
        **VOLATILE**, so a request written before the client writer
        matches the server request reader can be silently dropped.
        `service.cpp` now gates the first blocking `call_raw` write on
        `dds_get_publication_matched_status(writer).current_count > 0`.
        The async split path buffers the wire request in `ClientState`
        and flushes from `service_send_request_raw` /
        `service_try_recv_reply_raw` once the match appears, preserving
        non-blocking semantics. Verified locally with `ctest -R
        nros_rmw_cyclonedds_service_roundtrip`; stock ROS 2 interop
        tests still fail and remain separate Phase 117/171 work.
- [~] **171.0.b — Actions.** **Native C + Rust + C++ LANDED +
      runtime-verified 2026-05-21.** Both pieces built; design below.
      Native action e2e:
      - **C** (`examples/native/c/cyclonedds/action-{server,client}`):
        goal → accept → feedback → **result** `[0,1,1,2,3,5,8,13,21,34,55]`.
      - **Rust** (`examples/native/rust/cyclonedds/action-{server,client}`):
        goal → accept → feedback `[0]`→`[0,1,1,2,3,5,8,13]`. The client
        warms up discovery (~3 s spin) before the first `send_goal`,
        mirroring the C client — `send_goal` is a service call and its
        request races the writer↔reader match otherwise.
      - **C++** (`5b9ee97bc`, `28e9e6502`, 2026-05-21 follow-up): BUILD fixed
        + cpp+cpp e2e.
        The cpp-FFI
        cross-package gap (`action_msgs/GoalInfo` → `unique_identifier_msgs`
        types, `E0425`) was the cpp FFI `include!()` of a dependency's
        `.rs` not being transitive; the example CMakeLists now flatten the
        transitive closure (builtin + unique_identifier_msgs + action_msgs).
        cpp action **server** and **client** now run goal→accept→feedback→result
        e2e (`[0,1,1,2,3,5,8,13,21,34]`). The client warms up discovery
        before the blocking send_goal. NOTE: this was not a "Phase 77"
        regression — the cpp cyclonedds client already uses executor-spin.

        **Deep diagnosis 2026-05-21 (resolved by `28e9e6502`; do NOT
        re-derive — instrument was reverted):**
        - Cross-impl matrix: C-server reply is delivered to C & cpp
          clients; cpp-server reply is delivered to neither — but the
          cpp **client** *does* receive (cpp-client+C-server returns `-1`
          = post-receive buffer check, not timeout). So the cpp client
          receive path itself works.
        - `send_goal` acceptance works because it uses the **blocking
          `call_raw`** path (`service_call_raw`, self-contained poll).
          `get_result`/feedback use the **async path**
          (`send_request_raw` + arena-polled `try_recv_*`). The async
          *service* path works for C and rust (`#52`), so it is not the
          backend in general.
        - Cyclone discovery trace: the get_result reply writer
          (`rr/.../get_resultReply`, `wr …:603`) **matches** the client
          reply reader (`proxy_reader_add_connection`), exactly like the
          working send_goal reply (`:203`). `service_send_reply` returns
          ok=true. `service_try_recv_reply_raw` **receives the reply and
          correlates it** (`got_seq==pending && got_guid==my_guid` true).
        - So the break was **above the Cyclone backend.** The actual root
          cause was C++ action result framing. `complete_goal_raw` stores
          result **fields** only; C++ `ffi_serialize` / `ffi_deserialize`
          use normal CDR buffers with a 4-byte header. The native Cyclone
          typed `GetResult_Response_` descriptor expects fields, so storing
          C++'s header-prefixed result corrupted the typed response path.
          Fix: `nros_cpp_action_server_complete_goal` strips the CDR header
          before storage, and `cpp_result_trampoline` re-adds the CDR header
          before user callback / result stash delivery.
        - **Follow-up 2026-05-21:** feedback was published but C++ delivery
          still exposed field bytes without the CDR header expected by
          generated `ffi_deserialize`, and Cyclone's generic
          `dds_stream_read_sample` path still could not build typed samples
          for nested dynamic feedback/status sequences. Fixed by restoring
          the CDR header in the C++ feedback trampoline/stream path and by
          adding a narrow native publisher bridge for Fibonacci
          `FeedbackMessage_` plus `GoalStatusArray_`; both volatile topic
          writes wait for a matched reader before sending. Fresh
          `examples/native/cpp/cyclonedds/action-{server,client}` build/run
          on 2026-05-21 prints feedback payload and result with
          `STATUS=0`.
        - Cross-impl pairings (rust↔C↔cpp) still need explicit validation;
          same-language native pairs are the supported/verified baseline.
        The deeper codegen fix (make cpp-FFI `include!()` transitive) is
        a separate follow-up.

      Two pieces, both required before any native cyclonedds action runs:

      **Piece 1 — sub-type descriptor lookup (the `-1` blocker). LANDED
      `3db736aa1`.** `register_action_server` (`executor/action.rs`)
      passes the bare action type `<pkg>::action::dds_::<A>_` to all
      three sub-services (send_goal, get_result, feedback); the cyclonedds
      service path appended `_Request_`/`_Response_` and looked up the
      wrong descriptor. Fixed backend-locally (no change to the
      backend-agnostic action contract → zenoh wire key untouched): the
      cyclonedds service/topic create derives the real sub-type from the
      entity keyexpr suffix — `/_action/send_goal` → `<A>_SendGoal_`,
      `/_action/get_result` → `<A>_GetResult_` (`service.cpp`),
      `/_action/feedback` → `<A>_FeedbackMessage_`
      (`descriptors.{hpp,cpp}` + `publisher.cpp` + `subscriber.cpp`).
      Pass-through for non-action entities; ctest 12/12 still pass. Dead
      until Piece 2 supplies the descriptors.

      **Piece 2 — descriptor synthesis.** `msg_to_cyclone_idl.py` +
      the cyclonedds branch handle `.msg`/`.srv` only. Actions need the
      rosidl-synthesized wrapper types, none of which `action2idl` emits
      (it produces only the base Goal/Result/Feedback structs — verified):
        - `<A>_SendGoal.srv`  = `unique_identifier_msgs/UUID goal_id` +
          `<A>_Goal goal` --- `bool accepted` + `builtin_interfaces/Time stamp`
        - `<A>_GetResult.srv` = `unique_identifier_msgs/UUID goal_id` ---
          `int8 status` + `<A>_Result result`
        - `<A>_FeedbackMessage.msg` = `unique_identifier_msgs/UUID goal_id` +
          `<A>_Feedback feedback`
      Plus cross-package descriptors the action server registers directly:
      `action_msgs::srv::CancelGoal_`, `action_msgs::msg::GoalStatusArray_`
      (→ `GoalStatus`, `GoalInfo`), and `unique_identifier_msgs::UUID`.
      Approach: have the cyclonedds branch synthesize the three wrapper
      `.srv`/`.msg` texts from each `.action` (the base structs via
      `action2idl`, or decompose the `.action` directly), feed them
      through the existing converter (the nested-`::action::`-ref
      mangling added in `b49b0b42e` already covers action scoped refs),
      and require the example to `nros_generate_interfaces(action_msgs)`
      + `(unique_identifier_msgs)` into the shared idl/gen root so the
      cross-package `#include`s resolve.

      **CRITICAL framing constraint (found 2026-05-21).** The wrapper
      descriptors must match the **nros action layer's** CDR framing, NOT
      stock `rmw_cyclonedds_cpp` — these two diverge, so native cyclonedds
      actions are nano-ros↔nano-ros only:
      - `action_core.rs::{read,write}_goal_id` frames `goal_id` as a CDR
        `sequence<octet>` (4-byte length 16 + 16 bytes), *not* the stock
        fixed `uint8[16]` UUID. The synthesized `*_Request_` / `*_Response_`
        / `*_FeedbackMessage_` IDL must use `sequence<octet> goal_id`.
      - send_goal / get_result go through the regular service path, so
        their `_Request_`/`_Response_` structs must inline the 16-byte
        service header (`rmw_writer_guid` + `rmw_sequence_number`) first,
        like `.srv` (the converter's `inject_service_header` does this).
        The feedback topic is a plain message — no header.
      - The codegen emits only base `Fibonacci_{Goal,Result,Feedback}_`
        structs (no wrapper structs); the action layer assembles
        goal_id + base via `CdrWriter` directly. So the wrapper IDL is
        `{header?, sequence<octet> goal_id, <Base> nested}` — derive it
        to match, then verify a real goal→accept→feedback→result
        round-trip before declaring done.
      - Also register `action_msgs::srv::CancelGoal_` +
        `action_msgs::msg::GoalStatusArray_` (literal names the action
        server uses) from `nros_generate_interfaces(action_msgs)`.

      Also: the action client has a documented blocking-`zpico_get` hang
      risk (zenoh path). Not runtime-verifiable until both pieces land.
      The c/cpp native action examples already exist + build (they fail
      only at the `-1` above); rust action examples are NOT created (no
      point until the descriptors register).
- [ ] **171.0.c — aemv8r regression.** Confirm the existing
      `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` (FVP one-board
      reference) still builds after the topic + service backend changes.

### 171.A — Rename `dds` → `cyclonedds` in code surface

Mechanical rename across every non-example reference. Run BEFORE
any deletion so the workspace stays buildable at every step.

**Mostly landed.** Phase 169's dust-dds retirement + Group A's
`f139a9df5`/`7216fbaff` scrub already deleted the `nros-rmw-dds` crate,
the `rmw-dds` Cargo feature, the `Rmw::Dds` enum variant, and the
`NROS_RMW_DDS` macro / `NANO_ROS_RMW=dds` cmake branch. This pass
finished the residual stale `dds`-as-RMW-identifier strings.

- [x] **171.A.1** Workspace `Cargo.toml` `nros-rmw-dds` aliases — gone
      (Phase 169 deleted the crate; `rmw-cyclonedds` feature group present).
- [x] **171.A.2** `Rmw::Dds` enum variant — gone (no `Rmw::Dds` /
      `rmw-dds` / `nros-rmw-dds` left in the Rust surface; verified by grep).
- [x] **171.A.3** Root `CMakeLists.txt`: RMW dispatch is
      zenoh/xrce/cyclonedds (no `dds` branch); fixed the stale
      `NANO_ROS_RMW` cache-var doc-string + the `Unknown NANO_ROS_RMW`
      error message that still listed `dds`. `NROS_RMW_DDS` macro absent.
- [x] **171.A.4** Integration shells: only a stale comment in
      `integrations/nuttx/Makefile` (`rmw-{...,dds}-cffi`) remained →
      `cyclonedds`. zephyr/esp-idf/px4/platformio carry no `dds` value.
- [x] **171.A.5** `book/src/`: renamed the `dds` RMW identifier in
      `internals/rmw-backends.md` (bridge example + registry slots),
      `reference/{nros-toml,cli}.md`, `getting-started/integration-{nuttx,
      zephyr}.md`. Historical "dust-dds retired" prose left intact. Also
      updated the `e.g. "dds"` doc-comment examples in `nros-c`/`nros-cpp`/
      `nros-node` Rust source (cbindgen regenerates the headers).
- [~] **171.A.6** Reserve `rmw-dds` as a `compile_error!` — N/A as
      written: Phase 169 *deleted* the feature rather than renaming it in
      place, so enabling `rmw-dds` already fails with "unknown feature".
      No alias to gate. Closing as not-needed.

**Files (touched).** Every file under the grep
`rmw-dds|rmw_dds|RMW_DDS|NROS_RMW.*dds|nros-rmw-dds` outside
`docs/roadmap/archived/` and `third-party/`.

### 171.B — Rename example-tree `dds` → `cyclonedds`

For each existing `examples/<plat>/<lang>/dds/` directory, decide
whether the example actually targets dust-DDS or whether the
example is platform-agnostic enough to retarget at Cyclone DDS:

- Examples that link `nros-rmw-dds` directly (the Rust dust-DDS
  staticlib) — these get **deleted** in 171.D once Cyclone DDS has
  a matching example.
- Examples that just point at "the DDS backend, whichever it is"
  via cmake / cargo feature — these get **renamed** in place.

- [x] **171.B.1** Surveyed every `examples/*/*/dds/` directory
      (10 dirs) — classification table in
      `tmp/phase-171-example-classify.md`. Outcome:
      - **rename** candidates (Cyclone-DDS-viable platforms):
        `native/{c,cpp,rust}` (6 cases each), `zephyr/rust` (8),
        `threadx-linux/rust` (2).
      - **keep as `rmw-cyclonedds`, build-gated** (decision
        2026-05-21 — *keep Cyclone DDS support targeted at bare
        metal*, do NOT delete): `qemu-arm-baremetal/rust`,
        `qemu-arm-freertos/rust`, `qemu-arm-nuttx/rust`,
        `qemu-esp32-baremetal/rust`, `qemu-riscv64-threadx/rust`.
        The `rmw-cyclonedds` feature stays *defined* on these cells
        as the target; the build is gated on the Cyclone DDS RTOS
        port (§171.C.gate). Until that lands the cell is feature-
        present-but-unbuilt — see
        `phase-175-cyclone-rust-example-build-path.md`. Reverses the
        earlier "delete" plan.
- [~] **171.B.2** Migrated the rust dds examples off the retired
      `nros-rmw-dds` onto `nros-rmw-cyclonedds-sys` (feature
      `rmw-cyclonedds`) for `native`, `qemu-arm-freertos`,
      `qemu-riscv64-threadx`, `threadx-linux` (commit `40efd9319`).
      The bare-metal RTOS cells keep the feature defined but are NOT
      built in the fixture matrices (Cyclone has no pure-cargo build;
      gated on §171.C.gate). zenoh-only fixture matrices + the defer
      rationale live in `phase-175`. Remaining B.2 work: the
      `examples/<plat>/rust/dds/` *directory* renames (the migration
      flipped the collapsed cells' features in place; the legacy
      nested `dds/` dirs still need the `git mv` per B.3).
- [~] **171.B.3** For the rename candidates: `git mv
      examples/<plat>/<lang>/dds/ .../cyclonedds/` + flip the
      backend to cyclonedds.
      - [x] **`native/c` + `native/cpp`** (the only tracked `dds/`
        example dirs left after 169) renamed → `native/{c,cpp}/cyclonedds`,
        cmake flipped `NANO_ROS_RMW dds` → `cyclonedds` + `project()`
        prefixes renamed. **Verified**: `just cyclonedds setup` built
        Cyclone DDS 0.10.5; native c + cpp `cyclonedds/talker` both
        compile + link clean against `-DCMAKE_PREFIX_PATH=build/install`.
        Surfaced + fixed a real build bug: a C app linking the C++
        `nros-rmw-cyclonedds` failed with undefined `operator
        new`/`delete` / `std::nothrow` — the C link driver omits the
        C++ runtime. Fixed by propagating `stdc++` through the
        `NanoRos` INTERFACE in the root cmake cyclonedds branch
        (non-APPLE).
      - [ ] **`native/rust`** + the RTOS rust `dds/` dirs are already
        gutted by Phase 169 (source removed; only untracked
        `generated/`+`target/` artifacts remain). No tracked source
        to rename — these collapse into 171.B.2 (delete) / 171.C
        (cyclonedds-staticlib re-add) follow-ups.
      - [ ] **`threadx-linux`** rust cyclonedds — gated on the
        `nros-rmw-cyclonedds-staticlib` Rust path (171.C.3).
- [x] **171.B.4** `examples/README.md` matrix: dropped the `dds`
      column entirely (dust retired in 169); RMW list updated to
      `zenoh / xrce / cyclonedds / uorb`. cyclonedds column reflects
      reality — native c/cpp = 6 (verified), zephyr c = 2 (pub/sub),
      zephyr cpp = 4+aemv8r, zephyr rust = 4, native/rust +
      threadx-linux/rust = pending (171.C.1/.3), RTOS cells empty
      (171.C.gate). Gap-themes section rewritten.

### 171.C — Complete the cyclonedds example matrix

Fill every `<plat>/<lang>/cyclonedds/` cell that Cyclone DDS can
build on. Each cell gets the canonical six-example set (talker,
listener, service-{server,client}, action-{server,client}) unless
the platform has a known constraint (Phase 118's empty-cell rule).

Target matrix (after rename + new cells):

| Platform               | Language | cyclonedds cell |
|------------------------|----------|-----------------|
| `native`               | c        | full 6          |
| `native`               | cpp      | full 6          |
| `native`               | rust     | full 6 (via `nros-rmw-cyclonedds-staticlib`) |
| `zephyr`               | c        | pub/sub ✓ + service ✓ (collapsed shape); actions ✗ 171.0.b |
| `zephyr`               | cpp      | pub/sub ✓ + service ✓ + `talker-aemv8r` (existing); actions ✗ 171.0.b |
| `zephyr`               | rust     | pub/sub ✓ + service ✓ (collapsed shape); actions ✗ 171.0.b |
| `threadx-linux`        | c        | full 6          |
| `threadx-linux`        | cpp      | full 6          |
| `threadx-linux`        | rust     | full 6 (via staticlib) |
| `qemu-arm-freertos`    | c        | full 6 (gated on Cyclone DDS FreeRTOS port — Phase 171.C.gate) |
| `qemu-arm-freertos`    | cpp      | full 6 (same gate) |
| `qemu-arm-freertos`    | rust     | full 6 (same gate) |
| `qemu-arm-nuttx`       | c        | full 6 (gated on Cyclone DDS NuttX port) |
| `qemu-arm-nuttx`       | cpp      | full 6 (same gate) |
| `qemu-arm-nuttx`       | rust     | full 6 (same gate) |
| `qemu-riscv64-threadx` | c, cpp, rust | full 6 each (gated on Cyclone DDS NetX-Duo BSD port) |
| `qemu-arm-baremetal`   | rust     | gated — Cyclone DDS needs a POSIX-ish runtime; likely won't fit |
| `qemu-esp32-baremetal` | rust     | same gate as baremetal |
| `esp32`                | rust     | full 6 IF Cyclone DDS esp-hal-compatible port lands (Phase 117 follow-up); otherwise empty cell with documented reason |
| `stm32f4`              | rust     | same gate as baremetal |
| `px4`                  | cpp      | (uORB-only, unchanged) |

- [~] **171.C.1** **`native` × {c,cpp,rust}**.
      - [x] **c + cpp**: full 6 each (talker / listener /
        service-{server,client} / action-{server,client}) — all 12
        compile + link clean against
        `-DCMAKE_PREFIX_PATH=build/install` (Cyclone DDS 0.10.5 from
        `just cyclonedds setup`). Verified 2026-05-20.
      - [x] **rust** — **171.C.1.rust. Talker + listener + service
        LANDED + runtime-verified; action deferred.**
        Per-cell status (2026-05-21):
        - **talker** (`b49b0b42e`) — publishes `std_msgs/Int32` at 1 Hz.
        - **listener** (`a17ad5ba5`) — subscribes `/chatter`; rust
          talker → rust listener e2e delivers 0..4 over the wire.
        - **service-{server,client}** — build clean against the
          AddTwoInts cyclonedds typesupport and e2e round-trip passes:
          native rust server + native rust client completed **4/4**
          calls (`5+3`, `10+20`, `100+200`, `-5+10`) on 2026-05-21.
          Fix: the Cyclone backend now abandons a stale non-blocking
          client `pending_seq` when the upper Rust/C layer has already
          timed out and cleared its own in-flight guard, so a slow first
          request no longer wedges all later calls.
        - **action-{server,client}** — NOT created. All-language action
          over cyclonedds is blocked first: the cyclonedds branch of
          `nros_generate_interfaces` only wires `.msg`/`.srv`
          descriptors, not `.action`, so even the c/cpp action examples
          build but fail at runtime (`register_action_* -> -1`). Wiring
          action-type descriptors is the prerequisite (tracked with the
          deferred 171.0.b action item).

        Architecture resolved
        2026-05-20: a pure-cargo `nros-rmw-cyclonedds-staticlib`
        (the original plan, mirroring `nros-rmw-zenoh-staticlib`)
        will NOT work.** The Cyclone backend's raw-CDR path
        (`src/sertype_min.cpp`) needs a per-message
        `dds_topic_descriptor_t`, which Cyclone's **idlc** emits at
        **cmake time** via `nros_generate_interfaces(<pkg>)` +
        `cmake/NrosRmwCycloneddsTypeSupport.cmake`. A pure-cargo
        build has no idlc step, so it cannot produce the typesupport
        the backend dereferences. (Contrast zenoh: zenoh-pico needs
        no per-message C typesupport, so its staticlib is pure
        Rust.)

        **Therefore native rust cyclonedds must be cmake-driven**,
        reusing the proven c/cpp path. The shape (matches the Zephyr
        rust cyclonedds collapse, §171.0):
        - rust crate as `[lib] crate-type=["staticlib"]
          name="rustapp"` exporting `#[no_mangle] extern "C" fn
          rust_main()` (Executor talker/listener/… loop + a
          `nros_rmw_cyclonedds_sys::register()` call);
        - per-example `CMakeLists.txt`: `corrosion_import_crate`
          the rust staticlib + `set(NANO_ROS_RMW cyclonedds)` +
          `add_subdirectory(<repo-root>)` +
          `nros_generate_interfaces(std_msgs …)` (emits the Cyclone
          IDL typesupport) + a tiny `main.c`/`main.cpp` calling
          `rust_main()` + `target_link_libraries(app rustapp
          NanoRos::NanoRos)` (NanoRos pulls cyclonedds + `libddsc` +
          `stdc++` with rpath from the root cmake cyclonedds branch).

        Net-new hybrid (corrosion rust-staticlib + cmake-time
        Cyclone typesupport). **Talker landed + build-verified
        2026-05-20** at `examples/native/rust/cyclonedds/talker/`:
        the `rustapp` staticlib + `corrosion_import_crate` +
        `nros_generate_interfaces` + `NanoRos::NanoRos` +
        `--allow-multiple-definition` recipe compiles + links clean.
        **The split-vtable hazard is handled** — `nm` confirms a
        single `T nros_rmw_cffi_register_named` (count = 1) and a
        single `Registry` slot in the binary; the `#[no_mangle]`
        REGISTRY collapsed the cross-language copies as designed.
        Remaining 5 rust cases (listener / service-{server,client} /
        action-{server,client}) replicate the talker mechanically.
        threadx-linux rust (171.C.3) inherits the same shape.

        **Runtime fix — LANDED for the rust cell 2026-05-20
        (`b49b0b42e`).** The native rust cyclonedds talker now builds,
        creates its publisher, and **publishes `std_msgs/Int32` on
        `/chatter` on a 1 Hz timer** (verified: `Published: 0..3` over a
        5 s run). Root causes + fixes, all in that commit:

        1. **Descriptors never generated.** `nros_generate_interfaces`
           emitted only the C/CDR bindings, not the idlc
           `dds_topic_descriptor_t` + static-init register TU, so
           `create_publisher::<Int32>` had no registered descriptor.
           Fix: a cyclonedds branch in `nros_generate_interfaces` that
           drives `nros_rmw_cyclonedds_generate_from_msg` per package
           and WHOLE_ARCHIVE-links the self-registration TUs.
        2. **idlc never ran.** `$<TARGET_FILE:CycloneDDS::idlc>`
           expanded to `""` in the example's scope (imported target is
           directory-scoped). Fix: resolve idlc to an absolute path
           cached at module load in `NrosRmwCycloneddsTypeSupport.cmake`.
        3. **Register-TU / idlc-header build race.** `OBJECT_DEPENDS`
           on the register TU.
        4. **Composite messages** (`Header`, `*MultiArray`) cross-`#include`
           sibling / cross-package IDLs and reference nested types.
           Fix: a shared package-nested IDL + gen root with `-I`,
           sibling-`.idl` gating, dependency-package ts-lib ordering,
           and `msg_to_cyclone_idl.py` now mangles nested member type
           refs to `dds_::<Type>_` (previously only the top struct was
           mangled → idlc crashed resolving `std_msgs::msg::MultiArrayDimension`).
        5. **Spin/timer starvation.** `session_drive_io` returned
           instantly on hosted POSIX, so the callback-less `spin_once`
           free-ran sub-µs and the runtime's `elapsed.as_micros()` timer
           credit truncated to 0 — timers never fired. Fix: `nanosleep`
           the timeout, matching the Zephyr branch's pacing.
        6. **C-driver link.** rust talker links `stdc++` last (opaque
           `-Wl` flag, dodging CMake dedup) to resolve the C++ backend's
           `std::nothrow`; the ts lib takes only the backend's INTERFACE
           include dirs (not the lib) so `libnros_rmw_cyclonedds.a` stays
           inside NanoRos's `--whole-archive` group.

        **C / C++ native cells — LANDED + runtime-verified 2026-05-21
        (`cc26c09f9`).** The earlier `nros_support_init -> -3` was an
        empty RMW registry: the backend self-registers via the
        `.nros_rmw_init` linkme section walker, but `nros-node` pulls
        `nros-rmw-cffi` with `default-features = false` and its
        `rmw-cffi` feature does not re-enable `linkme-register`, so on
        the C-API path the walker is the no-op stub and the section
        entry is never invoked. (The locator default is NOT the cause —
        an empty locator reproduced the same `-3`.) Fixes:
        - An `.init_array` constructor on the Cyclone backend (gated off
          Zephyr) registers it before `nros_support_init`, regardless of
          the walker. `register_named` is idempotent, so harmless when
          the walker is also live (Rust-API builds).
        - C++ examples declare `project(... LANGUAGES CXX C)` — idlc
          descriptors are C source, uncompilable in a CXX-only project.

        Verified: native c + cpp talkers publish `std_msgs/Int32` at
        1 Hz, and a **C talker → C listener cross-process run delivers
        over the wire (`Received: 1..5`)** — full data-plane e2e, all
        three languages. **C + C++ service e2e** also works
        (server↔client AddTwoInts: 10+20=30, 100+200=300, -5+10=5; first
        call races discovery then recovers). **C/C++ action examples
        build but fail at runtime** (`register_action_* -> -1`) — the
        cyclonedds branch wires `.msg`/`.srv` descriptors only, not
        `.action`; action-type descriptor wiring is the prerequisite
        (deferred 171.0.b). Unblocking the service/action *builds*
        required two fixes (`a17ad5ba5`): skip `wstring` interfaces
        (Cyclone 0.10.5 idlc crashes on wide-string; the full ROS
        `example_interfaces` from `AMENT_PREFIX_PATH` ships
        `WString[MultiArray]`), and default the backend CTest harness ON
        only when it is the top-level project (so an example's
        `add_subdirectory` no longer builds the backend's own fixtures).

        **Hazard to design around (the reason this is not a quick
        spike):** the `rustapp` staticlib pulls the **Rust** nros
        runtime (cargo `nros` → `nros-rmw-cffi`), while cmake's
        `add_subdirectory(<repo-root>)` `NanoRos` pulls the **C**
        nros runtime (`nros-c` → `nros-node` → `nros-rmw-cffi`).
        Both carry `nros-rmw-cffi`'s vtable storage + the
        `nros_rmw_cffi_register` symbol. Linking both into one
        binary risks duplicate-symbol errors or — worse — a SPLIT
        vtable (the C++ cyclonedds `register()` writes one copy, the
        Rust `Executor` dispatches against the other → silent
        no-op, the same failure shape as Phase 166.A's FreeRTOS
        dup-symbol and the cyclonedds C-link `stdc++` gap). The
        Zephyr rust path (§171.0) sidesteps this because its
        `NanoRos`-equivalent provides ONLY cyclonedds + `libddsc`,
        not the full nros-c runtime. The native cmake glue must do
        the same: link the rust `rustapp` for the nros runtime +
        ONLY the cyclonedds backend archive (`nros_rmw_cyclonedds` +
        `libddsc` + `stdc++` + per-msg typesupport) from cmake — NOT
        `nros-c`/`nros-node`. Verify with `nm` that
        `nros_rmw_cffi_register` + the vtable static resolve to a
        single definition before declaring the cell done.
- [~] **171.C.2** **`zephyr` × {c, cpp, rust}** — **largely landed in
      §171.0** (collapsed shape + `prj-cyclonedds.conf`, not a
      `cyclonedds/` subtree). Pub/sub done all three languages; services
      done Rust + C++ + C. Remaining: Zephyr actions all langs
      (171.0.b).
- [~] **171.C.3** **`threadx-linux` × {c, cpp, rust}** — Cyclone
      DDS over the NetX-Duo / NSOS BSD shim (`packages/drivers/nsos-netx`).
      C/C++ collapsed CMake now honors `-DNROS_RMW=cyclonedds`, and
      `threadx-linux build-fixtures` builds the Cyclone C/C++ cells
      when local Cyclone artifacts are installed. The native rust
      service-client round-trip blocker from 171.C.1 is fixed; remaining
      work is the cyclonedds staticlib path plus CMake wiring so Cyclone
      socket calls route through NSOS rather than host libc.
- [x] **171.C.4 / .5 / .6 — RTOS + bare-metal cells: WON'T-FIT /
      deferred (gate decision, 2026-05-20).** Cyclone DDS requires a
      hosted runtime — BSD sockets, threads, heap, libc. The gate
      (below) splits the cells:
      - **Bare-metal — WON'T-FIT** (`qemu-arm-baremetal`,
        `qemu-esp32-baremetal`, `esp32`, `stm32f4`): pure Cortex-M /
        esp-hal have no POSIX socket layer, no hosted libc. Cyclone
        DDS cannot run. Documented as intentionally-empty cells in
        `examples/README.md` (same rule as Phase 118).
      - **FreeRTOS / NuttX QEMU — DEFERRED-UPSTREAM**: a Cyclone DDS
        FreeRTOS+lwIP / NuttX port is an upstream-scale effort
        (socket-shim + config + heap budget). Not attempted here;
        empty cells until an upstream port lands.
      - **ThreadX (linux + riscv64) — DEFERRED behind 171.C.1.rust +
        NSOS**: technically the most plausible (NetX-Duo BSD shim
        gives Cyclone a socket API), but still needs the cyclonedds
        staticlib path + per-target socket wiring.
- [x] **171.C.gate** **Cyclone DDS RTOS port assessment — done.**
      Decision recorded inline above (171.C.4/.5/.6): bare-metal
      won't-fit; FreeRTOS/NuttX deferred-upstream; ThreadX deferred
      behind the staticlib path. No RTOS cyclonedds cells are filled;
      `examples/README.md` marks them empty with reasons. The
      end-to-end spike was unnecessary — the runtime requirement
      (hosted POSIX) is a hard gate that bare-metal targets cannot
      meet by construction.

**`no_std + no-alloc` discipline.** Each new Rust example:
`#![no_std]`, `heapless::*` only, static-arena message storage.
Each new C example: no `malloc` in user code, fixed `char[N]`
scratch buffers. Each new C++ example: `NROS_CPP_STD=OFF`,
freestanding C++14 only.

### 171.C.runtime — Cyclone topic-descriptor typesupport wiring (the real runtime fix)

**Problem (diagnosed, see 171.C.1):** native cyclonedds examples
build but `create_publisher`/`create_subscription` stall at runtime
because the per-message Cyclone `dds_topic_descriptor_t` is never
registered. `nros_generate_interfaces` emits only the CDR/C(++)
message bindings (`<pkg>__nano_ros_c` / `__nano_ros_cpp`), never the
idlc descriptor + the static-init `nros_rmw_cyclonedds_register_descriptor`
TU. The backend's own ctest passes only because it hand-rolls the
descriptor via `nros_rmw_cyclonedds_add_idl_library`.

**Fix — make `nros_generate_interfaces` emit + link the Cyclone
descriptor when `NANO_ROS_RMW STREQUAL "cyclonedds"`. LANDED 2026-05-20/21**
(`b49b0b42e`, `16fdfcef7`, `cc26c09f9`, `a17ad5ba5`, `e9f5f2b61`,
`e337f7600`, `5b9ee97bc`). The plan below shipped, with deviations noted
inline.

- [x] **171.C.runtime.1** Added the cyclonedds branch to
      `cmake/NanoRosGenerateInterfaces.cmake`. Deviation: drives
      `nros_rmw_cyclonedds_generate_from_msg` (not `add_idl_library`) per
      `.msg`/`.srv`/`.action`, with a shared package-nested IDL+gen root
      (`-I` cross-package includes), sibling/dep-package ordering, and
      nested-`::msg::`/`::action::` type-ref mangling in
      `msg_to_cyclone_idl.py`. `wstring` interfaces skipped (idlc 0.10.5
      crash).
- [x] **171.C.runtime.2** `${target}__cyclonedds_ts` force-loaded via
      `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>` on the message lib's INTERFACE;
      ts lib takes only the backend's INTERFACE include dirs (linking the
      backend lib would dedup it out of NanoRos's whole-archive group).
      `nm`-verified the descriptor + register symbols land.
- [x] **171.C.runtime.3** Re-smoked: native rust talker `Published: 0..3`;
      rust talker→listener `Received: 0..4`; C/C++ talker + service e2e;
      C action goal→result `[…55]`; rust action goal→feedback. (A
      dedicated nextest harness was not added — verified by hand +
      backend ctest 12/12.)
- [x] **171.C.runtime.4** Scaffold/`-3` resolved. Root cause was NOT the
      locator (an empty locator reproduced `-3`) — it was the empty RMW
      registry: the C-API path's `.nros_rmw_init` walker was a no-op
      (`linkme-register` off), so the cyclonedds backend self-registers
      via an `.init_array` constructor now (`cc26c09f9`).
- [x] **171.C.runtime.5** Replicated: talker/listener/service across
      {c,cpp,rust}; native actions C+Rust+C++ e2e (cpp get_result fixed in
      `28e9e6502`). `threadx-linux` (171.C.3) and Zephyr actions
      (171.0.b) still pending.

**Acceptance:** a native cyclonedds talker+listener pair exchanges
`std_msgs/Int32` end-to-end (and ideally interops with stock
`ros2 topic echo` under `RMW_IMPLEMENTATION=rmw_cyclonedds_cpp`,
reusing the backend's existing `ros2_pubsub_e2e.sh` harness shape).

### 171.D — Deletion follow-ups left over from Phase 169

Most dust-DDS deletion (crates + submodule + workspace refs) is
**Phase 169's job**. By the time 171 starts, those are gone. The
items below are the lingering paperwork that surfaces after the
rename:

- [ ] **171.D.1** Delete the `compile_error!` aliases from 171.A.6
      after one minor-version release — kept for one release so
      out-of-tree consumers using the old `rmw-dds` feature name
      get a clear error rather than a missing-feature failure.
- [ ] **171.D.2** Update `book/src/internals/rmw-backends.md` host-
      language policy table — drop the dust-DDS row, leave the
      "retired Phase 169" footnote.

### 171.E — `no_std + no-alloc` audit on `nros-rmw-cyclonedds`

The wrapper package itself (not Cyclone DDS core) is freestanding
C++14 today. Tighten the audit:

- [x] **171.E.1** Grep `packages/dds/nros-rmw-cyclonedds/` for
      every `std::vector`, `std::string`, `std::shared_ptr`,
      `std::unique_ptr`, `new` / `delete`. Replace with `nros::`
      equivalents or stack-allocated fixed-capacity types where
      possible. Audit result: production wrapper has no STL containers
      or smart pointers; remaining `new` / `delete` sites are scalar
      per-session/per-entity state behind the C ABI's `void *`
      backend handles plus `SertypeMin` helpers. Removing those
      requires an ABI storage change, so they are documented rather
      than replaced in this pass.
- [x] **171.E.2** Document remaining `alloc`-touching call sites
      (Cyclone DDS's own API takes `dds_qos_t*` from
      `dds_create_qos()` which `malloc`s internally — that's
      transparent to nano-ros's wrapper but document the
      transitive allocation budget per-platform). See
      `packages/dds/nros-rmw-cyclonedds/README.md` "Freestanding /
      Allocation Audit".
- [x] **171.E.3** Add a CI check that
      `nros-rmw-cyclonedds` compiles with
      `-fno-exceptions -fno-rtti -fno-threadsafe-statics` on every
      target — same flags Phase 117 already uses, but make the
      assertion explicit. The backend target now carries all three
      flags in `target_compile_options`; `just cyclonedds build-rmw`
      and any in-tree `add_subdirectory` consumer inherit the check.

### 171.F — Acceptance + cleanup

- [ ] **171.F.1** `just ci` clean from root.
- [ ] **171.F.2** `rg -i "dust[ -_]dds|nros[-_]rmw[-_]dds\b"` 
      returns only hits under `docs/roadmap/archived/` (historical)
      and `book/src/changelog.md`-style files (history).
- [ ] **171.F.3** `examples/README.md` matrix updated: `dds` column
      gone, `cyclonedds` column populated per 171.C target.
- [ ] **171.F.4** `book/src/internals/rmw-backends.md` policy table
      updated.
- [ ] **171.F.5** Archive Phase 117 once 117.X.1–117.X.5
      stock-RMW interop slices are done (separate from this
      phase but enabled by the rename).
- [ ] **171.F.6** Archive Phase 166.F — dust-DDS Xtensa actor
      deadlock — as "won't-fix, dust-DDS retired".

---

## Files (touched)

Code:
- `Cargo.toml` (workspace members + aliases)
- `CMakeLists.txt` (NANO_ROS_RMW branch)
- `packages/core/nros/src/rmw.rs` (or wherever `Rmw::Dds` lives)
- `packages/dds/nros-rmw-dds/` (delete)
- `packages/dds/nros-rmw-dds-staticlib/` (delete)
- `packages/dds/nros-rmw-cyclonedds/` (audit; possibly add Rust
  staticlib sibling per 171.C.1 if Rust users need a static
  archive)
- `packages/testing/nros-tests/tests/dds_ros2_interop.rs` (rewrite)
- `packages/testing/nros-tests/tests/server_available_e2e.rs`
  (rewrite)
- `packages/testing/nros-tests/tests/zephyr.rs` (drop the
  `NROS_RMW_DDS` test branch)
- `third-party/dust-dds/` (submodule delete)

Examples (per 171.B + 171.C tables — likely 60-100 directories
moved or created).

Docs:
- `examples/README.md` (matrix)
- `book/src/internals/rmw-backends.md` (host-language policy)
- `book/src/user-guide/rmw-backends.md` (user-facing RMW pick
  guide)
- `book/src/concepts/comparison-vs-microros.md` (drops the
  dust-DDS reference)
- Every starter page that mentions the `dds` RMW option:
  `book/src/getting-started/{freertos,zephyr,native,esp32,
  threadx,bare-metal,integration-*}.md`.

Integrations:
- `integrations/zephyr/Kconfig` (`CONFIG_NROS_RMW` choice)
- `integrations/esp-idf/Kconfig.projbuild`
- `integrations/nuttx/Kconfig`
- `integrations/platformio/library.json`
- `integrations/px4/module-template/CMakeLists.txt`

---

## Acceptance criteria

- [ ] `cargo check --workspace --all-features` clean — no
      `nros-rmw-dds` / `dust-dds` references in the resolved
      graph.
- [ ] `git ls-files | rg "dust|nros-rmw-dds"` returns hits only
      under `docs/roadmap/archived/` (history) and `CHANGELOG`-style
      files.
- [ ] `examples/<plat>/<lang>/cyclonedds/` populated per the
      171.C matrix; every cell either has the canonical 6 examples
      OR an entry in the "Intentionally empty cells" section of
      `examples/README.md` explaining why.
- [ ] `just test-all` passes — every test that previously depended
      on dust-DDS either passes against Cyclone DDS (renamed +
      rewired) or is removed.
- [ ] Every new Rust example declares `#![no_std]` and contains
      no `extern crate alloc` line.
- [ ] Every new C example contains zero `malloc` / `free` in user
      code (Cyclone DDS internal allocation is acceptable).
- [ ] Every new C++ example compiles with `-fno-exceptions -fno-rtti`
      and `NROS_CPP_STD=OFF`.
- [ ] `book/src/internals/rmw-backends.md` host-language policy
      table no longer lists dust-DDS.

---

## Notes

- **Why C++ for `nros-rmw-cyclonedds`, not Rust?** RMW backend
  host-language policy (Phase 117 era): backend's host language
  matches its underlying library's native language unless overridden.
  Cyclone DDS is C++ (the OMG DDS API binding) on top of a C core.
  A Rust adapter is feasible but adds maintenance burden for zero
  capability gain — same wire format, same DCPS semantics, just a
  thicker FFI surface. Rust users consume Cyclone DDS via a
  `nros-rmw-cyclonedds-staticlib` C wrapper (analogous to
  `nros-rmw-zenoh-staticlib`).
- **Why retire dust-DDS now?** Three pressures converge:
  1. Phase 166.F (Xtensa LX7 Actor deadlock) blocks Phase 117
     close-out and the fix path is "rewrite the actor mailbox" or
     "swap critical-section impl" — both are large investments in
     a backend we'd otherwise retire.
  2. Cyclone DDS is the reference DDS for ROS 2 — wire-compat with
     stock `rmw_cyclonedds_cpp` is THE interop goal. dust-DDS
     interop has been "close, with footnotes" for a year.
  3. Maintaining two DDS backends doubles the test matrix +
     security review surface for no capability gain.
- **What about `nros-rmw-dust-dds` as a separate optional
  external crate?** Out of scope. If a downstream wants to keep
  dust-DDS support they can fork pre-169 and maintain it; nano-ros
  itself ships one DDS backend.
- **`no_std + no-alloc` in `nros-rmw-cyclonedds`** is bounded by
  Cyclone DDS's own allocation model. The wrapper crate can be
  alloc-free but Cyclone DDS's `dds_create_qos()`, sample
  allocation, etc. allocate internally — document the per-platform
  allocation budget rather than pretending it's zero.
- **Submodule deletion** (`third-party/dust-dds/`) is the only
  destructive `git rm` in this phase; double-check no
  downstream-fork branches are pinned at that submodule tree.
