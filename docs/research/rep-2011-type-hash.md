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
4. **Serialize to "hashable JSON"** via libyaml in flow style: flow
   sequences `[…]`, flow mappings `{…}`, all keys and string values
   double-quoted, numerics plain, `width=-1` so nothing wraps. Object
   key order is fixed by the writer:
   - `TypeDescription`: `type_description`, `referenced_type_descriptions`
   - `IndividualTypeDescription`: `type_name`, `fields`
   - `Field`: `name`, `type`
   - `FieldType`: `type_id`, `capacity`, `string_capacity`, `nested_type_name`
5. **SHA-256** the UTF-8 byte buffer (the libyaml char buffer minus its
   trailing NUL — `buffer_length - 1`). No leading/trailing newlines
   are added by the writer; libyaml emits a single stream without a
   final break (`YAML_NO_BREAK`).
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

- **`example_interfaces/srv/AddTwoInts`** — `.srv` is `int64 a; int64
  b\n---\nint64 sum`. Top-level has three NESTED_TYPE fields; three
  referenced messages, alphabetically: `_Event`, `_Request`,
  `_Response`. The `_Event` message is the standard
  `service_msgs/msg/ServiceEventInfo`-plus-bounded-sequences shape
  generated by rosidl, so the input is non-trivial. Skeleton:

  ```
  {"type_description":{"type_name":"example_interfaces/srv/AddTwoInts","fields":[
    {"name":"request_message", "type":{"type_id":1,…,"nested_type_name":"example_interfaces/srv/AddTwoInts_Request"}},
    {"name":"response_message","type":{"type_id":1,…,"nested_type_name":"example_interfaces/srv/AddTwoInts_Response"}},
    {"name":"event_message",   "type":{"type_id":1,…,"nested_type_name":"example_interfaces/srv/AddTwoInts_Event"}}]},
   "referenced_type_descriptions":[ <Event>, <Request>, <Response>, service_msgs/msg/ServiceEventInfo, builtin_interfaces/msg/Time ]}
  ```

  Full canonical text and final `RIHS01_…` value depend on the exact
  `_Event` shape emitted by Jazzy's `rosidl_generator_type_description`
  (which itself follows `service_msgs`). Compute on a Jazzy host:
  `ros2 interface hash example_interfaces/srv/AddTwoInts`. Both
  reference values should land as constants in
  `packages/codegen/interfaces/` fixture data once verified.

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
