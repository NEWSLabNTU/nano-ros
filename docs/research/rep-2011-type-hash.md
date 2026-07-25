# REP-2011 type hash (RIHS01) — notes for nano-ros codegen

**Last updated:** 2026-05-17 (Phase 41.1).
**Scope:** what `rosidl-codegen` has to do to emit REP-2011-conformant
`TYPE_HASH` constants alongside generated Rust messages. Distilled from
`ros2/rcl` (`rcl/src/rcl/type_hash.c`),
`ros2/rosidl` (`rosidl_runtime_c/src/type_hash.c`,
`rosidl_generator_type_description/__init__.py`,
`rosidl_runtime_c/include/rosidl_runtime_c/type_description/field_type__struct.h`).
The REP page on `ros.org/reps/rep-2011.html` 404s; canonical text is the
source.

## 1. Canonical type description

The hash input is a `TypeDescription` struct, not the `.msg` text. The
struct is fully closed under nested-type reference:

```
TypeDescription {
  type_description:                IndividualTypeDescription,
  referenced_type_descriptions:    [IndividualTypeDescription],   // DAG-closed
}

IndividualTypeDescription {
  type_name:  string,           // e.g. "std_msgs/msg/Int32"
  fields:     [Field],          // .msg declaration order, preserved
}

Field {
  name:           string,
  type:           FieldType,
  default_value:  string,       // empty if none — included in the description
                                // struct, but rcl strips it before hashing
}

FieldType {
  type_id:          uint8,      // enum below
  capacity:         uint64,     // array len or sequence upper bound; 0 for scalar
  string_capacity:  uint64,     // string upper bound; 0 unless BOUNDED_*
  nested_type_name: string,     // FQ name for nested refs; "" otherwise
}
```

`type_id` is the numeric `rosidl_runtime_c__type_description__FieldType`
enum, **emitted as decimal in the hashed text** (not symbolic). Selected
values:

| id | name | id | name |
|----|------|----|------|
| 1 | NESTED_TYPE | 13 | CHAR |
| 2 | INT8 | 14 | WCHAR |
| 3 | UINT8 | 15 | BOOLEAN |
| 4 | INT16 | 16 | BYTE |
| 5 | UINT16 | 17 | STRING (unbounded) |
| 6 | INT32 | 18 | WSTRING |
| 7 | UINT32 | 19 | FIXED_STRING |
| 8 | INT64 | 21 | BOUNDED_STRING |
| 9 | UINT64 | 49–70 | `*_ARRAY` (fixed) |
| 10 | FLOAT | 97–118 | `*_BOUNDED_SEQUENCE` |
| 11 | DOUBLE | 145–166 | `*_UNBOUNDED_SEQUENCE` |
| 12 | LONG_DOUBLE | — | — |

`(scalar_id + 48)` → ARRAY, `+ 96` → BOUNDED_SEQUENCE, `+ 144` →
UNBOUNDED_SEQUENCE. So `int32[4]` is `type_id=54, capacity=4`;
`string<=20[]` is `type_id=161, string_capacity=20`. `default_value` is
present on the struct but `rcl_type_description_to_hashable_json` does
not emit it — hashes are independent of defaults.

## 2. Normalization & hash algorithm

1. **Build the DAG.** Walk every `.msg` field; for every `NESTED_TYPE`
   recurse and collect into `referenced_type_descriptions`. Self ref of
   the top-level type is **not** repeated in `referenced`.
2. **Sort referenced types alphabetically by `type_name`.** This is the
   only sort in the pipeline. Field order inside each
   `IndividualTypeDescription` is the source `.msg` order — never
   re-sorted.
3. **Service top-level.** A service description's `type_description` has
   three fields: `request_message`, `response_message`,
   `event_message`, each `type_id=NESTED_TYPE` pointing at
   `<Srv>_Request`, `<Srv>_Response`, `<Srv>_Event`. The three nested
   messages live in `referenced_type_descriptions` (still
   alphabetically sorted with any transitive deps). Actions are the
   same shape with seven members.
4. **Serialize to "hashable JSON".** **CORRECTED 2026-07-25 (phase-304 W1b) —
   the earlier "no whitespace / `width=-1`" claim was WRONG.** The normative
   generator is `rosidl_generator_type_description.calculate_type_hash`:
   ```python
   json.dumps(hashable_dict, ensure_ascii=True, indent=None,
              separators=(', ', ': '), sort_keys=False)
   ```
   i.e. a **space after every `,` and `:`** (`', '` / `': '`), ASCII-escaped
   strings, dict-INSERTION key order (not sorted). `default_value` IS stripped
   from every field before hashing (the one thing the earlier notes got right).
   Object key order (insertion):
   - `TypeDescription`: `type_description`, `referenced_type_descriptions`
   - `IndividualTypeDescription`: `type_name`, `fields`
   - `Field`: `name`, `type` (default_value removed)
   - `FieldType`: `type_id`, `capacity`, `string_capacity`, `nested_type_name`
5. **SHA-256** the UTF-8 bytes of that `json.dumps` string. Confirmed
   byte-exact against a live Jazzy install (see §3).
6. **Format as `RIHS01_` + 64 lowercase hex chars.** Total length 71.
   Prefix bytes 4..6 are the version (`"01"`), bytes 0..4 the literal
   `"RIHS"`, byte 6 the separator `'_'`. Version 1 is the only
   currently-defined version.

REP-2011's claim that the canonical form is a "newline-delimited
per-field text" is informal; the *normative* form is the libyaml-flow
JSON described above. SHA-256 only sees that one buffer.

## 3. Reference hashes

No reachable ROS install with REP-2011 support: `/opt/ros/humble` is
Humble, which predates type hashes (`ros2 interface hash` is unknown —
the subcommand exists only on Iron+). `find /opt/ros -name '*.json'
-path '*type_description*'` returns nothing; `find /opt/ros -name
'*.sha256.txt'` returns nothing. The values below are therefore
**unverified** — they're the canonical-JSON-then-SHA256 derivation, to
be confirmed against a Jazzy install (or `rcl_calculate_type_hash`)
before they're committed to fixture tests.

- **`std_msgs/msg/Int32`** — `.msg` is `int32 data` (and only comments).
  Canonical JSON (one line, no leading whitespace):

  ```
  {"type_description":{"type_name":"std_msgs/msg/Int32","fields":[{"name":"data","type":{"type_id":6,"capacity":0,"string_capacity":0,"nested_type_name":""}}]},"referenced_type_descriptions":[]}
  ```

  `RIHS01_<sha256-of-the-above>` — **unverified**, compute via Jazzy
  `ros2 interface hash std_msgs/msg/Int32` to confirm.

  **CONFIRMED against live Jazzy (phase-304 W1b, 2026-07-25):** the real value
  is `RIHS01_b6578ded3c58c626cfe8d1a6fb6e04f706f97e9f03d2727c9ff4e74b1cef0deb`
  (`/opt/ros/jazzy/share/std_msgs/msg/Int32.json` → `type_hashes[0]`). The
  `rosidl_codegen::rihs` engine reproduces it byte-for-byte once the §4
  separators fix (spaces) was applied — the capture loop found the bug. The
  canonical hashable string (spaced separators, default_value stripped):
  ```
  {"type_description": {"type_name": "std_msgs/msg/Int32", "fields": [{"name": "data", "type": {"type_id": 6, "capacity": 0, "string_capacity": 0, "nested_type_name": ""}}]}, "referenced_type_descriptions": []}
  ```

  Other confirmed references (committed in
  `packages/testing/nros-tests/fixtures/ros-editions/jazzy/hashes.txt`):
  - `std_msgs/msg/Header` (nested Time + string) →
    `RIHS01_f49fb3ae2cf070f793645ff749683ac6b06203e41c891e17701b1cb597ce6a01`
  - `builtin_interfaces/msg/Time` →
    `RIHS01_b106235e25a4c5ed35098aa0a61a3ee9c9b18d197f398b0e4206cea9acf9c197`
  - `geometry_msgs/msg/Twist`, `sensor_msgs/msg/Imu` — captured; used when the
    codegen wiring (W1b c) lands the per-type assertions.
  The engine reproduces Int32 + Header + Time byte-for-byte (rihs unit tests).

## 3a. Service `_Event` synthesis — CONFIRMED against live Jazzy (2026-07-25)

Captured from `ros:jazzy-ros-base`, `std_srvs/srv/SetBool`
(`share/std_srvs/srv/SetBool.json` → `type_description_msg`). This is the
COMPLETE recipe a codegen must reproduce; every id/capacity below is verbatim.

**Top-level `<pkg>/srv/<Srv>`** — three `NESTED_TYPE` (id 1) fields, in **source
order** (never sorted):

| field | type_id | nested_type_name |
|-------|---------|------------------|
| `request_message`  | 1 | `<pkg>/srv/<Srv>_Request` |
| `response_message` | 1 | `<pkg>/srv/<Srv>_Response` |
| `event_message`    | 1 | `<pkg>/srv/<Srv>_Event` |

**`referenced_type_descriptions`** (whole DAG, sorted alphabetically by
`type_name` at hash time — for SetBool that order is Time, ServiceEventInfo,
`_Event`, `_Request`, `_Response`):

- **`<Srv>_Request`** — the request fields, verbatim from the `.srv` (before `---`).
- **`<Srv>_Response`** — the response fields (after `---`).
- **`<Srv>_Event`** — SYNTHESIZED, always this shape:
  | field | type_id | capacity | nested_type_name |
  |-------|---------|----------|------------------|
  | `info`     | 1  | 0 | `service_msgs/msg/ServiceEventInfo` |
  | `request`  | 97 | 1 | `<Srv>_Request`  |
  | `response` | 97 | 1 | `<Srv>_Response` |

  `97` = NESTED_TYPE(1) + BOUNDED_SEQUENCE_OFFSET(96); `capacity=1` — i.e.
  `<Srv>_Request[<=1]` / `<Srv>_Response[<=1]`.
- **`service_msgs/msg/ServiceEventInfo`** — a FIXED built-in (Iron+). Embed its
  canonical ITD as a codegen constant (do NOT depend on the `service_msgs` pkg
  being ament-resolvable):
  | field | type_id | capacity | nested_type_name |
  |-------|---------|----------|------------------|
  | `event_type`       | 3  | 0  | `""` |
  | `stamp`            | 1  | 0  | `builtin_interfaces/msg/Time` |
  | `client_gid`       | 51 | 16 | `""` |
  | `sequence_number`  | 8  | 0  | `""` |

  **Parser gotcha:** the `.msg` line is `char[16] client_gid`, but rosidl maps
  `char` → `uint8` (id 3), so `char[16]` → id `51` (UINT8 + ARRAY_OFFSET 48),
  NOT id 13 (CHAR). nano-ros must apply the same `char`→uint8 mapping (mostly
  moot here since ServiceEventInfo is embedded as a constant).
- **`builtin_interfaces/msg/Time`** — transitive dep of ServiceEventInfo.

**Golden Jazzy hashes** (committed as fixtures):
```
std_srvs/srv/SetBool          RIHS01_abe9e4bb6b41b40e6789712c00ec8871923e089af3f667a79992a428cff2da0a
std_srvs/srv/SetBool_Event    RIHS01_3c4c20015afb4303eafd347b1d6a786f171a89c814726961a9593ef10df878cf
std_srvs/srv/SetBool_Request  RIHS01_c62fbb99d94e1b25e8ef9e109f9581956bb1b3361a45a4e5810c36a90d29932e
std_srvs/srv/SetBool_Response RIHS01_d0814e7f7b4880ab77e9c57426c7aa1562ab69f11eef8e2e968812f9cbd0b059
service_msgs/msg/ServiceEventInfo RIHS01_41bcbbe07a75c9b52bc96bfd5c24d7f0fc0a08c0cb7921b3373c5732345a6f45
```
Each of the 5 (`<Srv>`, `_Request`, `_Response`, `_Event`, ServiceEventInfo) is a
SEPARATE hash — rcl hashes the whole DAG once per top-level type_name.

## 3b. Action synthesis — CONFIRMED (tf2_msgs/action/LookupTransform)

Actions expand to **six** top-level `NESTED_TYPE` fields (source order), two of
which are nested SERVICES (each themselves carrying the `_Request`/`_Response`/
`_Event` triad from §3a):

| field | nested_type_name |
|-------|------------------|
| `goal`               | `<A>_Goal` |
| `result`             | `<A>_Result` |
| `feedback`           | `<A>_Feedback` |
| `send_goal_service`  | `<A>_SendGoal`  (service: Request = `UUID goal_id` + `<A>_Goal goal`; Response = `bool accepted` + `builtin_interfaces/Time stamp`) |
| `get_result_service` | `<A>_GetResult` (service: Request = `UUID goal_id`; Response = `int8 status` + `<A>_Result result`) |
| `feedback_message`   | `<A>_FeedbackMessage` (= `UUID goal_id` + `<A>_Feedback feedback`) |

`UUID` = `unique_identifier_msgs/msg/UUID` (`uint8[16] uuid`). The two nested
services (`_SendGoal`, `_GetResult`) each synthesize their own `_Event` per §3a,
so an action closes over ServiceEventInfo + Time + UUID as well.

**Golden Jazzy hashes** (full tree captured, committed as fixtures):
```
tf2_msgs/action/LookupTransform              RIHS01_0b8adf6bc0b5958879e3265b41a457e03558fe523890a81252c70eba97a82c5d
tf2_msgs/action/LookupTransform_SendGoal     RIHS01_4646ff5706c86b04d0a1098d329951a9731a7517e16702905de6073cc72c8530
tf2_msgs/action/LookupTransform_GetResult    RIHS01_3cd1715751899e3167b5aec3e4ac194f7da9e8493a77285f9f6a4c914f5e8b24
tf2_msgs/action/LookupTransform_FeedbackMessage RIHS01_10cb89922dcb103e95c185d43e8a5efa3d38db996f733151a2f3e2ac133f4839
```
(and `_Goal`/`_Result`/`_Feedback`/`_SendGoal_{Request,Response,Event}`/
`_GetResult_{Request,Response,Event}` — see `fixtures/ros-editions/jazzy/`.)

**Synthesis order for the engine:** build `_Request`/`_Response` from the parsed
`.srv`; synthesize `_Event` (§3a); synthesize the top-level; hand the DAG (plus
the embedded ServiceEventInfo/Time constants) to `build_type_description` +
`rihs01`. Actions layer the two service triads + the Goal/Result/Feedback/
FeedbackMessage wrappers on top, reusing the service path for `_SendGoal` /
`_GetResult`.

## 4. What nano-ros needs

`rosidl-codegen` (the `cargo nano-ros generate-rust` path) must (a)
build the `TypeDescription` DAG from the parsed `.msg`/`.srv`/`.action`
AST it already has, (b) sort `referenced_type_descriptions`
alphabetically by `type_name`, (c) emit the libyaml-flow canonical
string with the fixed key order in §2, (d) `sha2`-hash it, and (e)
emit `pub const TYPE_HASH: nros_core::TypeHash = TypeHash::new(*b"…64
hex…");` on the generated struct's `impl` block (alongside the existing
`MessageT` trait impls in the emitted `mod.rs`). Gate the work behind a
`type-hash` cargo feature on `nros-codegen` (default-on once verified;
off lets us ship pre-Iron compatibility). The runtime needs a
matching `nros_core::TypeHash` type — 32 bytes plus a `pub const
PREFIX: &str = "RIHS01_"` formatter — so RMW shims can pass the value
verbatim into the zenoh key expression (`<domain>/<topic>/<type>/<hash>`)
and replace today's `TypeHashNotSupported` placeholder. The Cyclone /
XRCE backends additionally want the raw 32-byte array to fill
`rmw_type_hash_t` for upstream wire-compat (Phase 117.X).
