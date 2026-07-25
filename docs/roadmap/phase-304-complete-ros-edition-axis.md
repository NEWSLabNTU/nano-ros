# Phase 304 — Complete the ROS edition axis

Implements **[RFC-0056](../design/0056-ros-edition-axis.md)** (the ROS-edition
axis / per-distro interop profile). Coordinates the two field-phases —
**[phase-41](phase-41-iron-type-hash-support.md)** (RIHS01 type hash) and
**[phase-303](phase-303-xcdr2-interop.md)** / **[RFC-0055](../design/0055-wire-encoding-xcdr2-extensibility.md)**
(wire encoding) — into a finished axis, adds the **unified selection** and the
**multi-distro test method**, and extends the enum beyond `humble`/`iron`. Roots:
issue **#0267** (the encoding half) and the discovery-rejection risk phase-41
flagged (the type-hash half). Test lanes plug into
**[RFC-0051](../design/0051-test-matrix-architecture.md)** (the edition becomes a
matrix axis).

## Status (2026-07-25)

**W3 LANDED** (extend the enum). **W2 core + W4 started** (this commit):
`[system].ros_edition` is declared + resolved + typo-guarded; `nros_tests::ros2`
is distro-parametric; the Tier-A capture script exists. **W1 not started** (the
biggest functional gap; needs the capture run). The axis is **partially
built**: `RosEdition {Humble, Iron}` +
`--ros-edition` CLI arg + `ros-humble`/`ros-iron` cargo features + a runtime
keyexpr branch exist, but (a) `RosEdition::type_hash()` returns a **placeholder**
for Iron (`RIHS01_<64×0>`, not computed), (b) codegen selection (the CLI arg)
and runtime selection (the cargo feature) are **disconnected** — two knobs for
one axis that can silently disagree, and (c) only `humble` is exercised.

## The problem: the CI/dev host installs only Humble

`/opt/ros/` has **humble only**; `nros_tests::ros2` hard-codes
`/opt/ros/humble/setup.bash` in its availability checks (though
`ros2_env_setup(distro)` is already distro-parameterized), and the pinned
`rmw_zenoh` (1.7.2) is Humble-compat. So "test on Iron/Jazzy" needs a way to
reach those distros without a host-wide install. Two tiers:

### Tier A — offline, no distro install (codegen correctness)

Everything a distro changes in the GENERATED artifacts is capturable **once**
into committed fixtures and tested in-repo with no ROS runtime:

- **RIHS01 reference hashes** — `ros2 interface hash <type>` on the distro
  yields the `RIHS01_<sha256>` for each type. Capture a small set
  (`std_msgs/Int32`, a nested-struct type, a service), commit as fixtures, and
  assert the Rust REP-2011 computation (phase-41 W1) reproduces them
  byte-for-byte. This is exactly the "collect reference hashes from Iron/Jazzy"
  TODO phase-41 §41.1 left open. **No live peer needed.**
- **Per-distro `rosidl_adapter` IDL** — the `nros-msg-to-idl` parity contract
  (`tests/parity.rs`) is Humble-captured today; capture the Jazzy/Iron
  `rosidl_adapter` output per fixture and assert the emitter matches the
  SELECTED edition's reference (extensibility annotation, if any, is a per-distro
  fixture — this is where the phase-303 W1 finding gets pinned down per distro).
- **Per-distro `.msg` deltas** — the interface set differs; capture the target
  distro's `.msg` for the fixtures.

Capture source: a throwaway **`osrf/ros:<distro>`** container (`ros:iron-ros-base`,
`ros:jazzy-ros-base`) run once by a `scripts/ros/capture-edition-fixtures.sh`
helper — the container is a capture tool, not a test dependency.

### Tier B — live wire interop (needs the distro at test time)

Actual discovery + on-wire decode against a real ROS 2 node of the target
distro. Two options; the matrix picks per-lane:

- **B1 — container peer (recommended, no host install).** The interop test
  spins an `osrf/ros:<distro>` container as the ROS 2 peer (`ros2 topic echo` /
  a small node) and nano-ros (host) talks to it over the network (zenoh router
  or DDS). Needs the distro's matching `rmw_zenoh` (version per distro) inside
  the container. Isolated, CI-friendly, coexists with the humble host.
- **B2 — multi-`/opt/ros` install.** apt-install `ros-iron-*` / `ros-jazzy-*`
  alongside humble (they coexist under `/opt/ros/`), generalize the
  `nros_tests::ros2` hard-coded `/opt/ros/humble` availability checks to the
  requested distro, and source the right overlay. Simplest on a dev box; a
  provisioning change for CI.

**Skip discipline (RFC-0051 / the fail-loud rule):** an edition lane whose
distro (container image or `/opt/ros/<distro>`) is absent must
`nros_tests::skip!` — never a bare pass. Only `humble` runs unconditionally.

## Work items

### W1 — real RIHS01 computation (drives phase-41)

Replace `RosEdition::type_hash()`'s Iron placeholder with the REP-2011 RIHS01
(`docs/research/rep-2011-type-hash.md`): add `sha2` to `rosidl-codegen`,
implement the canonical type-description form + SHA-256, gate the real hash on
`iron`+ (keep the Humble `TypeHashNotSupported`).

**W1 engine LANDED (2026-07-25):** `rosidl_codegen::rihs` — the pure engine.
`FieldTypeDesc`/`FieldDesc`/`IndividualTypeDescription`/`TypeDescription` (the
REP-2011 type-id enum + array/sequence offsets), `to_hashable_json` (libyaml-flow
canonical form, fixed key order, referenced-types sorted / fields kept), and
`rihs01` (SHA-256 → `RIHS01_<64hex>`). Verified: the canonical JSON matches the
research doc's documented `std_msgs/msg/Int32` reference BYTE-FOR-BYTE; the
engine's Int32 hash is snapshot-locked
(`RIHS01_22ff2de7…f99b6`) as a regression guard. rihs.rs is clippy-clean.

**W1b (a) LANDED (2026-07-25) — AST → TypeDescription:** `field_type_desc`
(primitive/string/bounded-string → scalar id; array/sequence/bounded-sequence →
element base + `+48/+96/+144` offset with capacity/string_capacity; namespaced →
NESTED_TYPE + `pkg/msg/Name`), `message_to_individual` (fields only, source
order), and `build_type_description` (DAG closure over nested refs via a
caller-supplied `resolve` callback — de-duped, loud error on an unresolvable
ref). Tested: the primitive/array/`string<=20[]`→161/bare-bounded-string→21/
nested mappings + a two-level DAG closure + the unresolvable-ref error. Bounded
strings in collections + wstrings are best-effort pending the (b) Jazzy
confirmation.

**W1b (b) CONFIRMED against LIVE Jazzy (2026-07-25):** ran the capture against a
`ros:jazzy-ros-base` container and read the rosidl type-description `.json`s. The
loop FOUND A REAL BUG — the hashable form is NOT compact (the research doc
guessed wrong): `calculate_type_hash` uses
`json.dumps(separators=(', ', ': '), ensure_ascii=True, sort_keys=False)` with
`default_value` stripped. Fixed `to_hashable_json` (spaced separators + ASCII
escaping); the engine now reproduces the REAL Jazzy hashes byte-for-byte for
`std_msgs/msg/Int32` (`RIHS01_b6578ded…`), `std_msgs/msg/Header` (nested Time +
string, `RIHS01_f49fb3ae…`), and `builtin_interfaces/msg/Time`
(`RIHS01_b106235e…`) — all locked as unit-test assertions. Reference hashes
committed in `fixtures/ros-editions/jazzy/hashes.txt`; the research doc §4 is
corrected. The capture script now reads the `.json` type descriptions (there is
no `ros2 interface hash` subcommand — the doc's claim was also wrong).

**W1b (c) LANDED (2026-07-25) — codegen wiring:** `generate_package`
(`rosidl-bindgen`) now computes the per-message `TYPE_HASH` via
`compute_msg_type_hash` — Humble keeps the `TypeHashNotSupported` placeholder
(`edition.uses_type_hash()` gate); Iron/Jazzy/Rolling build the REP-2011
`TypeDescription` DAG and emit the real `RIHS01_<hash>`. The emission fns
(`generate_nros_message_package` / `generate_nros_inline_message`) now take a
`type_hash: &str` (hash decided by the caller) instead of an `edition`.
Nested-type closure: `generate_package` resolves **same-package** nested types
itself from `package.share_dir`; **cross-package** types come from a
caller-supplied `MsgResolver` (`ws sync` + `cargo-nano-ros` build one over the
ament interface index; self-contained/Humble paths pass `no_cross_pkg_resolver`).
An unresolvable nested type is a **HARD error** — never a wrong or placeholder
hash on the wire. Codegen-level assertions in `generator.rs` pin the real Jazzy
Int32 (`b6578ded…`, flat) and Header (`f49fb3ae…`, nested Time via resolver)
hashes byte-for-byte, plus the Humble-placeholder and fail-loud paths.

- *Accept (W1b):* ✅ the engine reproduces the Tier-A captured reference hashes
  for the fixture set byte-for-byte (rihs unit tests + the codegen-level
  `generator.rs` tests); `humble` unchanged (placeholder).

**W1b (c) REMAINING:** service/action `TYPE_HASH` still emit the placeholder on
Iron+ (the `_Event` synthesis — a service's top-level description has three
`NESTED_TYPE` members `<Srv>_{Request,Response,Event}`, where `_Event` is the
`service_msgs/ServiceEventInfo`-plus-bounded-sequences shape rosidl generates —
is a distinct REP-2011 sub-problem, not yet built). And the runtime keyexpr +
liveliness token still read `RosEdition::type_hash()` (the compile-time
placeholder) rather than the codegen-baked per-type `TYPE_HASH`; wiring the
generated constant through to the wire is W2b's baking step.

### W2 — unify edition selection (`[system].ros_edition`, RFC-0056 open-Q1)

Declare the edition ONCE in `system.toml [system].ros_edition` (default
`humble`) and lower it — like RMW (RFC-0031) — to (a) the codegen
`--ros-edition`, (b) the `ros-<edition>` cargo feature on the board/umbrella,
and (c) the `generated/<edition>/` interface dir. Kills the codegen↔runtime
disconnect (baked type_hash must match the runtime keyexpr tail).

- *Accept:* a `[system].ros_edition = "iron"` workspace bakes the Iron type_hash
  AND builds with `ros-iron` AND resolves `generated/iron/` — no hand-set
  feature; a mismatch is impossible by construction. A missing/`humble` value is
  byte-identical to today.

**W2 core LANDED (2026-07-25):** `SystemHeader.ros_edition: Option<String>` +
`SystemHeader::ros_edition() -> Result<RosEdition>` (absent ⇒ humble; unknown ⇒
HARD error — typo guard, never a silent fallback). `nros codegen-system`
resolves + validates + records it at bake (a bad `[system].ros_edition` fails
loudly). Unit-tested (`ros_edition_resolves_with_humble_default_and_typo_guard`).

**W2b REMAINING (the lowering):** thread the resolved edition into (a) the
message-gen `--ros-edition` default (baked type_hash), (b) the `ros-<edition>`
cargo feature on the generated entry crate (runtime keyexpr format — closes the
last end of the codegen↔runtime disconnect), and (c) the `generated/<edition>/`
interface dir. The DECLARATION + typo guard exist; the auto-lowering is next.

### W3 — extend the enum: `jazzy` / `rolling` — **LANDED (2026-07-25)**

`RosEdition` gained `Jazzy`/`Rolling` + a single `RosEdition::parse` /
`as_str` / `uses_type_hash` API (every CLI parse site — `ws sync`,
`generate`, `generate-px4` — routes through it, so a new distro is one arm).
`ros-jazzy`/`ros-rolling` cargo features added across the 6 forwarding
Cargo.tomls (nros-rmw-zenoh → staticlib → nros-node → nros → nros-c → nros-cpp);
`nros-rmw-zenoh::keyexpr`'s "modern edition" branch (was `ros-iron`) now keys on
`any(ros-iron, ros-jazzy, ros-rolling)` so jazzy/rolling append the RIHS01
type-hash tail like iron; the `nros` umbrella mutual-exclusion `compile_error!`
covers all four. type_hash for iron/jazzy/rolling is the RIHS01 form with a
PLACEHOLDER digest (W1 computes the real one — the FORMAT is right, the digest
is not).

- *Verified:* `RosEdition` unit tests (parse/as_str/round-trip/type_hash for all
  four); `nros-rmw-zenoh --features ros-jazzy` compiles + keyexpr resolves;
  `nros --features ros-humble,ros-jazzy` fails with the mutual-exclusion error;
  rosidl-codegen (403) + nros-msg-to-idl parity (93) suites green; humble default
  byte-identical.

### W4 — multi-distro test infrastructure

- `scripts/ros/capture-edition-fixtures.sh` — capture Tier-A fixtures from an
  `osrf/ros:<distro>` container (RIHS01 hashes, `rosidl_adapter` IDL, `.msg`).
- Generalize `nros_tests::ros2`: replace the hard-coded `/opt/ros/humble`
  availability checks with the requested distro; add the Tier-B1 container-peer
  harness (or the B2 install probe), skipping when absent.
- RFC-0051 test matrix gains the **edition** axis; edition lanes are declared,
  precondition-skip when the distro is unavailable.

- *Accept:* an Iron or Jazzy interop lane PASSES against a container peer (B1) or
  an installed distro (B2), and skips loudly when neither is present; the
  offline Tier-A fixture tests run in CI with no ROS runtime.

**W4 STARTED (2026-07-25):**
- `scripts/ros/capture-edition-fixtures.sh <iron|jazzy|rolling>` — captures the
  Tier-A RIHS01 hashes (`ros2 interface hash`) + rosidl_adapter `.msg` from a
  throwaway `osrf/ros:<distro>-ros-base` container into
  `packages/testing/nros-tests/fixtures/ros-editions/<distro>/`. Syntax-checked +
  distro-guarded (humble/unknown rejected). Running it (a ~1 GB pull) produces
  the golden values **W1** asserts against — the capture is the W1 prerequisite.
- `nros_tests::ros2` is now distro-parametric: `is_ros2_distro_available(distro)`
  (bare-identifier-guarded) generalizes the hard-coded `/opt/ros/humble` check;
  `is_ros2_available()` keeps the humble default. The container-peer harness
  (B1) + the RFC-0051 edition matrix axis are the remaining W4 pieces.

### W5 — encoding field per edition (coordinates phase-303 W1b)

Make the wire-encoding default a profile field the edition selects: `humble` →
XCDR1 (byte-identical to today, parity intact); `jazzy` → the phase-303 XCDR2
path (W2–W4 there). **Blocked** on the phase-303 W1 distro capture (RFC-0055
open-Q5 / #0267) — do not build the XCDR2 encoder before the live-peer evidence.

- *Accept:* deferred to phase-303 W1b acceptance; recorded here so the axis's
  encoding field is not forgotten.

## Non-goals

- Runtime multi-edition in one binary (compile-time exclusive, like RMW/platform).
- Full package-set parity with every distro — nano-ros generates the interfaces a
  workspace declares, per edition.

## Done when

`[system].ros_edition` selects the edition end-to-end (codegen type_hash +
cargo feature + interface dir, W2); the RIHS01 computation matches captured
Iron/Jazzy references (W1); an Iron or Jazzy interop lane passes against a
container peer (W4/B1); and a `humble` build is byte-identical to today
throughout. The encoding field (W5) closes with phase-303.

## References

- Design: RFC-0056 (axis / profile), RFC-0055 (encoding), RFC-0051 (test matrix),
  RFC-0031 (declared-and-lowered selection — the model for W2).
- Phases: phase-41 (RIHS01 type hash — W1 drives it), phase-303 (XCDR2 encoding
  — W5 coordinates it).
- Issue: #0267 (the encoding half's root-cause + the demo-distro capture that
  unblocks W5).
- Research: `docs/research/rep-2011-type-hash.md` (RIHS01 canonical form).
