# Phase-555 W2: rmw-wire review, verdicts and decision brief

Reviewer: nros 139, 2026-11-13. This covers the 20 RFCs in `area: rmw-wire`.
The mechanical fixes are in this PR, and every one of the 20 has
`last-reviewed: 2026-11`. Every RFC keeps `area: rmw-wire`: no reviewer found
a better area. 0031 could equally sit in build-toolchain-codegen; it stays
where it is.

Each verdict below is the reviewer's recommendation. A verdict that changes a
status or a design waits for the ruling on its item in the brief.

## Verdicts

| RFC | Title (short) | Verdict | Evidence |
|---|---|---|---|
| 0005 | RMW layer design | **archive**, after its live section moves (B1) | Most of it is the phase-33/34 crate-rename plan. The `zpico-platform-<board>` crates and `packages/zpico/` are gone (`ls packages/rmw/zenoh`). Only §"Receive-buffer sizing" is still a live contract. |
| 0006 | Portable RMW / platform interface review | **supersede-by 0054** (B2) | R1 says the Rust structs are canonical and cbindgen writes the header. R2 and RFC-0054 decided the opposite: the headers are canonical, with committed bindgen output. R5's `abi_version` never landed (`rmw_vtable.h`, and phase-121 rejected it for the platform ABI). R6's Python smoke test does not exist. |
| 0009 | Bridge topic-forwarding | **amend** (B3) | What ships is phase-267's `PubSubBridge` poll-and-pump runner over `nros-bridge.toml` (`main_macro.rs:2944-2948`, `bridge/src/config.rs:274,437`), which the RFC rejected for generated code. The generated `register_bridges` has no caller (`nros-build/src/emit.rs:76-91`). |
| 0011 | PX4 uORB RMW | **amend** (B4) | §§2-5 and 8-9 describe the retired Rust stack (`topics.toml`/`phf`, `nros_px4::run`). The backend is C++ (`topic_registry.cpp`, `px4_callback_glue.cpp:79-87`). |
| 0028 | E2E safety protocol | **amend** (B5) | Built by phase-35. §7's layer map does not match the code: the CRC lives in `nros-rmw-zenoh` (`shim/mod.rs:1931-2005`). The protection exists on zenoh only. |
| 0031 | RMW selection and lowering | **amend** (B6) | The declared-to-lowered principle and the precedence ladder are live (`SystemToml::resolved_rmw`). §"CycloneDDS exception" is contradicted by the code (`nros-board-linux/Cargo.toml:102,114`), and the single-package-leaf and selection-package rows are retired (RFC-0071 D13, RFC-0121). |
| 0035 | RMW vtable ABI | **amend** (B7) | The mandated `abi_version` never landed. The return codes were renumbered to positive values (`rmw_ret.h:76-186`), and slots were removed (`required_rx_bytes`) without the superseding RFC that rule 3 requires. |
| 0038 | Zero-copy transport | **amend** (B8) | D2 and the Acceptance section say in-place dispatch is the default, but 9 of 11 subscription entry points copy (`registration_observer.rs:16-23`; RFC-0100). `BufferStrategy` still exists (`arena.rs:586`). |
| 0055 | XCDR2 wire encoding | **amend** (B9) | Open question 5 records "Decision: edition-gated". That contradicts the RFC's own correction and the code: `XCDR2 = false` (`nros-c/src/cdr.rs:20`) and `TYPE_EXTENSIBILITY = 0` (`dynamic_type.rs:56`). |
| 0056 | ROS edition axis | keep | The 2026-10 amendment holds. Open questions 2 and 3 are answered: RIHS01 is computed in-tree (`rihs.rs`), and Jazzy uses XCDR1. Mechanical. |
| 0069 | Action payload envelope | keep, then **Accepted → Stable** (B10) | All acceptance boxes are ticked. The executor always splices the header (`arena.rs:3474-3498`), and the sniff is `#[cfg(test)]` only. |
| 0071 | RMW backend descriptor | keep, then **Stable** once phase-545 closes (B11) | D1-D19 are in the code (`nros-rmw.toml` in four backends and `acme_rmw`, `RmwIndex`, `adapter_crate_name`, the `rmw-agnostic` gate, `out_of_tree_rmw_e2e.rs`). |
| 0075 | Zenoh router provenance | keep, then **Stable** (B12) | Phase-362 is complete, D1-D5 are in the code, and `third-party/zenoh` is gone. |
| 0080 | CAN link for zenoh-pico | **amend**, then Stable (B13) | The body still describes the rejected unicast `tx_id`/`rx_id` link with bitrate keys. What was built is multicast `id`/`match`/`mask` (fork `link/multicast/can.c:203`), and RFC-0109 D10 refuses the bitrate keys. |
| 0081 | CAN link for zenoh-rs | keep, then **Stable** (B14) | Phase-378 is complete. The remaining W7 is open issue 1822. |
| 0082 | Demonstrable CAN stack | keep, then **Stable** (B15) | Phase-387 W0-W7 are done (`docker/can-demo/`). |
| 0083 | CAN unicast over ISO-TP | keep, with §3 narrowed (B16) | Phases 393 and 394 are complete. The MCU ports via the vendored isotp-c are not selected by any build. |
| 0088 | Serialization format provider | keep, with a D2 amend, then **Stable** (B17) | Phase-421's W1-W6 landed. Every decision is in the code, apart from the bridge `converter` key. |
| 0109 | Network config in three layers | keep, then **Stable** (B18) | Phases 514 and 546 are done. D9-7's per-endpoint provenance in `nros config explain` looks unbuilt. |
| 0110 | Driving zenoh-pico 1.10's executor | keep | Verified in the code (`zpico.c:146-151,3476`, `nros-zpico-build lib.rs:441,1483`, `zpico.h:576`). |

Mechanical fixes made in this PR:

- **Dead and moved paths, and renamed functions,** across the 20.
- **Body status lines** that contradicted the frontmatter (0110, 0028).
- **Missing `amends:` links:**
  - 0109 → 0004, 0009, 0080, 0081, 0083, 0098, 0103;
  - 0042 → 0006, 0031;
  - 0086 → 0049, 0071;
  - 0094 → 0065, 0071, 0087.
- **Phase links:**
  - 0028 is now tracked by phase-35, whose doc names it back, and its line
    leaves the phase-tracking baseline;
  - 0009 adds phase-267;
  - 0071 adds phase-545.
- **Stale code comments:**
  - `zephyr/Kconfig` (the zenoh-pico CAN link is CAN FD only);
  - `nros-c/src/cdr.rs` and Cyclone's `Cargo.toml` (XCDR1 on every edition);
  - the `docker/can-demo` README's branch name;
  - an api-parity ledger row that credited RFC-0005 with RFC-0028's E2E check.

## Decision brief

Each item gives the question, the measured facts, the options and a
recommendation.

### Supersede, archive or rewrite

**B1. RFC-0005: archive?**

- **Facts:** the crate table and directory layout are the phase-33/34 plan,
  and that crate split is superseded by RFC-0034's `nros-platform-*`. The
  live part is §"Receive-buffer sizing": the per-slot `take` /
  `rx_buffer_hint` contract.
- **Options:**
  - (a) Move that section into RFC-0035, archive 0005, and repoint the
    citations (CLAUDE.md:32, ARCHITECTURE.md:273,
    book/src/extending/add-a-transport.md:233).
  - (b) Keep it, amended to say the crate split is superseded by 0034.
- **Recommend (a).**

**B2. RFC-0006: supersede by RFC-0054?**

- **Facts:** see the verdict table (R1 contradicts R2 and 0054; R5 and R6
  were never built). The RMW slot contract lives in RFC-0035.
- **Options:** (a) supersede by 0054, and repoint README.md:53 and the
  citations named in B1; (b) amend R1, R5 and R6 in place.
- **Recommend (a).**

**B3. RFC-0009: which bridge is the design?**

- **Facts:**
  - The shipped runner forwards every declared topic when `topics` is empty
    (`planner.rs:1071-1079`), and is one-way unless `bidirectional`
    (`plan.rs:127-131`).
  - The generated relay sets no QoS, though the RFC says it uses the declared
    interface's.
  - RFC-0088 D3's format check lives in `PubSubBridge` (`bridge/src/lib.rs:7-17`).
- **Options:**
  - (a) Amend the RFC to record the data-driven runner and its defaults, and
    delete the dead `register_bridges` generator.
  - (b) Wire `register_bridges` in, as the RFC says.
- **Recommend (a):** it ships, phase-267 tests it, and it carries the format
  check.

**B4. RFC-0011: rewrite for the C++ backend?**

- **Facts:**
  - The registry is fixed-capacity and derived per image.
  - The service slots return `UNSUPPORTED` (`service.cpp:27`).
  - Wake-up is driven by WorkItem `ScheduleNow()`.
  - RFC-0039:241 already rejected merging this into 0039.
  - Phase-549 may bring back a Rust/C module path through RFC-0039.
- **Options:** (a) rewrite §§2-5 and 8-9 for the C++ backend and keep the
  Rust design as history; (b) archive 0011 and let 0039 carry PX4.
- **Recommend (a).**

### Design behaviour

**B5. RFC-0028: E2E protection is zenoh-only and fails OPEN elsewhere.**

- **Facts:**
  - The default `take_validated` returns `crc_valid: None` (`nros-rmw
    traits.rs:3768-3784`; `cffi lib.rs:1027-1031`), and `is_valid()` accepts
    `None` (`safety.rs:80-81`).
  - So a Cyclone or XRCE image built with `safety-e2e` reports every sample
    valid without checking anything.
  - `nros/safety-e2e` does not forward to the backend feature
    (`nros/Cargo.toml:212-216`).
  - §7's "no change to nros-rmw-zenoh" is wrong as built.
- **Options:**
  - (a) Document E2E as zenoh-only.
  - (b) Refuse `safety-e2e` at build time on a backend with no carrier.
  - (c) Design a DDS carrier, such as an inline QoS or a payload footer; §4
    Option B rejected those for interop.
- **Recommend (b) plus (a),** plus an "As built" section replacing §7. Today
  an unsupported backend says "valid", which is the worst of the three
  outcomes.

**B6. RFC-0031: strike the retired lowering mechanics?**

- **Facts:** §"CycloneDDS exception" and the single-package-leaf and
  selection-package rows (around 0031:55 and 135-148) describe shapes that
  RFC-0071 D13 and RFC-0121 retire. 0071 says it replaces 0031's rule but
  declares no amendment.
- **Options:** (a) amend 0031 and add `amends: [rfc-0031]` to 0071;
  (b) supersede 0031 by 0071 plus 0121.
- **Recommend (a):** the declared-to-lowered principle and the ladder are
  live.

**B7. RFC-0035: drop the `abi_version` mandate and relax the removal rule?**

- **Facts:**
  - There is no `abi_version` field.
  - Backends are always built from source in the same configure: no prebuilt
    archive ships (0071:416), and the out-of-tree acme backend compiles
    against the checkout's header.
  - Three gates hold the ABI: `check-abi-bindings`, `rmw-abi-shape` and
    `rmw-slot-table`.
  - Slots have already been removed when nothing called them.
  - README.md:60's one-liner still advertises "frozen 34-slot … +
    `abi_version`" (the table has 78 slots).
- **Options:**
  - (a) Implement `abi_version` at offset 0, as written.
  - (b) Amend to "one checkout = one ABI, enforced by gates; a slot nothing
    calls may be removed", then promote to Stable.
  - (c) Leave it.
- **Recommend (b),** and fix the README line with it.
- Separately, `implements-tracked-by: [phase-237]` looks unrelated to the
  vtable. Unverified; check it with the amendment.

**B8. RFC-0038: in-place dispatch is per entry point, not the default.**

- **Facts:**
  - 9 of 11 subscription entry points copy: generic, `.message_info()`,
    `.safety()`, borrowed views, and four of the five C/C++ entry points.
  - The borrowed-view case contradicts D4.
  - The ABI added two slots, not one: `subscription_supports_in_place` and
    `process_raw_in_place`.
  - The pooled receive was deferred (phase-231:151), so "independent of
    subscriber count" is unmet.
- **Options:**
  - (a) Amend D2 and the Acceptance section to "in-place where the entry
    point reports it", as `open_subscription` already does (RFC-0100,
    phase-457 W3); record the pool as deferred; and file an issue for D4's
    borrowed views.
  - (b) Open a phase to make the other nine in-place.
- **Recommend (a).**

**B9. RFC-0055: close it out?**

- **Facts:**
  - The correction says the encoding is chosen per type, never per edition.
  - The code is XCDR1 and FINAL everywhere.
  - No per-type `@appendable` signal exists.
  - §4's negotiation is unbuilt.
- **Options:**
  - (a) Mark it Stable-parked: strike Q5's decision, record that §2's
    "default Appendable" is refuted, and list the per-type signal as future
    work.
  - (b) Keep it as a Draft.
- **Recommend (a).** It also covers a follow-up: the stale "edition-appropriate"
  comment in the codegen pack `packs/c/_field.jinja:349` is not changed here,
  because it is emitted into generated files.

**B16. RFC-0083: narrow §3 to what is built?**

- **Facts:** §3 says FreeRTOS, NuttX, ThreadX and bare metal get ISO-TP from
  the vendored isotp-c. But `Z_FEATURE_LINK_ISOTP_VENDORED` is hard-coded to
  0, commented "no nano-ros build selects it" (`nros-zpico-build/src/lib.rs:513-515`),
  and nothing tracks the MCU ports.
- **Options:** (a) narrow §3 to Linux and Zephyr and list the MCU ports as
  future work; (b) file an issue or phase for the MCU ports; (c) both.
- **Recommend (c).**

**B17. RFC-0088: amend D2, and decide the orphaned bridge key.**

- **Facts:**
  - D2 says the format `u8` is "assigned by the build, image-local". In
    practice providers author it (`nros-serdes-packed/nros-serdes.toml:29`)
    and a gate enforces global uniqueness (check-serdes-descriptors S2).
  - The D3 bridge `converter` key is a `TODO(phase-421 W4)` in
    `bridge/src/config.rs:137-145`, although W4 is archived. So D2's "the
    format crosses images in `nros-bridge.toml`" is not true yet.
- **Options:** (a) amend D2 to "authored per provider, unique in the tree,
  meaningful within an image", and file an issue for the converter key;
  (b) build the per-image allocator.
- **Recommend (a),** then Stable.

**B18. RFC-0109: D9-7 provenance.**

- **Facts:** `nros config explain` prints only `[system] locator`
  (`cmd/config.rs:708`), not the file and line each endpoint came from. I did
  not run the command.
- **Options:** (a) build D9-7; (b) move it to "Not decided here".
- **Recommend:** promote to Stable with (b) if no phase picks it up. Extend
  the header prose's "Amends" list to match the frontmatter.

### Status promotions

These items have no design content; each asks only whether to change the
status.

- **B10.** 0069: Accepted → Stable. The README index row still says Draft.
- **B11.** 0071: Stable once phase-545 closes. First rule D17 and D18, which
  are marked "open to review" (0071:~792, ~855), and record that D14 resolves
  D5's "today none carry package.xml".
- **B12.** 0075: Draft → Stable.
- **B13.** 0080: amend first, folding the RESOLVED section and RFC-0109 D10
  into §§2, 4.2, 5 and 6, then Stable with "hardware unvalidated" stated.
- **B14.** 0081: Draft → Stable. Also state that §5's "skip with a clear
  message" applies to tests in the zenoh fork, not to nano-ros's no-skip rule.
- **B15.** 0082: Draft → Stable. Record §4.4 (the container builds from a
  local fork checkout) as superseded by the upstream PRs zenoh#2757,
  zenoh-pico#1298 and zenoh-c#1322.

### Untracked work found (file or decline)

- **CAN hardware validation.** Phase-377 W6 and phase-394's tier 3 are in no
  open phase; phase-512 runs the CAN hub board over serial only.
- **The RFC-0088 bridge `converter` key** (B17).
- **nros-cpp's non-test `payload_has_cdr_encap` sniff**
  (`nros-cpp/src/action.rs:1042-1059`). RFC-0069 recorded this kind of check
  as a bug in the executor. It is probably unreachable. Recommend an issue to
  splice the header unconditionally.
- **Section markers in `rmw_vtable.h`.** Slots 22-35 fall under the "Status
  events" heading in RFC-0035's generated table, because the header has no
  markers before the loan slots (:605) or `ping_session` (:846). Adding them
  regenerates the table and touches the C ABI header, so it is not done in
  this docs PR.
- **The stale uORB backend README** ("K.4.0 scaffold, every slot
  UNSUPPORTED").
- **CLAUDE.md** names `report_tiers_above_transport`, but the code has
  `report_sched_tasks_above_transport` (`nros-board-freertos/src/entry.rs:1296`).
  That is a user-owned file, so this PR leaves it unchanged.

## Pairs checked

Pairs where the verdict changed something:

- **T2 0004-0109, 0098-0109, 0103-0109:** the `amends` link was missing on
  0109; it is now added. The amended RFCs already carry their side in prose.
- **T2 0006-0042 and 0031-0042:** the `amends` link was missing on 0042; it
  is now added.
- **T2 0071-0086 and 0071-0094:** the `amends` links were missing on 0086 and
  0094; they are now added.
- **T2 0006-0035:** see B2.
- **T2 0031-0071:** see B6.
- **T2 0071-0098:** consistent; RFC-0071 D13 records the renamed adapter.
- **T2 0080-0081:** the two RFCs complement each other. 0080 §4.2's
  "priority-blind" holds for pico only, and the zenoh-rs blocker is issue
  1822. Each RFC now points at the other.

Pairs that were citations only and need no change:

- **T1 0004-0031:** 0031 cites 0004 as its configuration home; no conflict.
- **T2 0035-0042:** 0042 keeps the vtable slot ABI unchanged.
- **T2 0065-0071:** consistent.
- **T2 0065-0088:** the D6 citation is updated.
- **T2 0071-0101:** a rejected alternative, so no conflict.
- **T2 0087-0109:** the provider format is cited, and no change is claimed.
- **T2 0088-0090:** "relates to", and no change is claimed.
