---
id: 1062
title: "`nano_ros_add_node` has two language readers that disagree — cmake
  expands SOURCES, the scanner reads the raw text, and the loser is a silent
  `C`"
status: open
type: bug
area: tooling
related: [issue-0939, issue-0641, rfc-0057]
---

## Symptom

Autoware Safety Island's `controller_pkg` is a C++ component. Its metadata
probe is generated against the **C** ABI seam and fails to link:

```
sync: source metadata — no producer for controller_pkg::controller
      (metadata probe build failed for `controller_pkg::controller` (exit 2):
...
probe_controller_pkg__controller.cpp:(.text+0x6a): undefined reference to
      `__nros_c_component_controller_pkg_create'
probe_controller_pkg__controller.cpp:(.text+0xa4): undefined reference to
      `__nros_c_component_controller_pkg_configure'
```

Those symbols come from `NROS_C_COMPONENT`. The component registers with
`NROS_COMPONENT`, which exports `__nros_component_factory_controller_pkg`.

Nothing else breaks. The image builds and runs; only the sidecar the probe
exists to produce goes missing, and `sync` carries on with a model that lacks
it. This is the same failure shape as issue 0939 — a probe that cannot link,
reported once and then tolerated — one layer further in.

## Cause

The emitter is right. `is_c_node` routes on `lang`, and `lang` arrives already
wrong.

A `nano_ros_add_node` declaration is read by **two** different readers, and
only one of them can see CMake variables:

```cmake
nano_ros_add_node(controller
  CLASS   controller_pkg::Controller
  SHAPE   rclcpp
  SOURCES ${_controller_sources})
```

**Reader 1 — cmake, at configure time.** `${_controller_sources}` expands to
real `.cpp` paths, `_nros_infer_lang` answers `cpp`, and the verb forwards
`LANGUAGE cpp` to `nano_ros_node_register`. Correct.

**Reader 2 — the Rust scanner, statically.** `parse_add_node_call`
(`workspace.rs`) reads `CMakeLists.txt` as *text*. It never expands anything,
so `sources` holds one token — the literal string `${_controller_sources}` —
and:

```rust
// Infer from the source extension, NOT the class: a C node's class still uses
// `::` (e.g. `c_talker_pkg::Talker`), so class-based inference mislabels it Cpp.
let language = infer_language_from_sources(&sources);
```

```rust
/// C++ if any source has a C++ extension, else C.
fn infer_language_from_sources(sources: &[String]) -> ComponentLanguage {
    for s in sources {
        if s.ends_with(".cpp") || … { return ComponentLanguage::Cpp; }
    }
    ComponentLanguage::C
}
```

No `.cpp` suffix is visible, so it falls off the end into `C`. That answer
reaches `metadata_refresh.rs`, which maps `ComponentLanguage::C → "c"`, which
routes the probe through the C-ABI seam.

The `else C` is doing two unrelated jobs: *"every source is a C file"* and
*"this list told me nothing"*. Only the first deserves an answer.

## Two defects, one symptom

**1. The default is silent, and it is the wrong shape.** Any `SOURCES` the
scanner cannot resolve — a variable, a generator expression, an empty list —
reads as C. The sibling path already learned exactly this lesson;
`infer_cmake_language` fails loudly and its comment cites issue 0641:

> An unrecognised value now falls back LOUDLY rather than silently, because a
> silent fallback is exactly what hid this: the declaration said one thing, the
> inference did another, and nothing printed.

`parse_add_node_call` never got that treatment. The third reader
(`parse_register_call`) already handles the empty case by deferring to the
class; only this one guesses.

**2. There is no way to say it.** `nano_ros_node_register` accepts
`LANGUAGE C|CPP|RUST`. `nano_ros_add_node` does not:

```cmake
cmake_parse_arguments(_NRN "TYPED" "CLASS;HEADER;SHAPE" "SOURCES;DEPLOY;CALLBACK_GROUPS" ${ARGN})
```

and the scanner has no `LANGUAGE` keyword either. So a package that knows the
guess is wrong cannot correct it — and passing `LANGUAGE CPP` today is worse
than useless: unmatched keywords land in `_NRN_UNPARSED_ARGUMENTS`, which the
verb appends to `_srcs`, so `LANGUAGE` and `CPP` become two source files.

## Fix

- Give `nano_ros_add_node` the `LANGUAGE` keyword its own callee already has,
  explicit value winning over inference, and teach the scanner to read it.
- Split "this list says C" from "this list says nothing". Only a source that
  actually carries a recognised extension is evidence; when none does, fall
  back to the class shape and **print why**, naming `LANGUAGE` as the fix.

The class fallback is still a guess — that is the point of printing it. A C
component whose class carries `::` and whose sources are hidden behind a
variable will guess wrong, out loud, with the remedy in the message.
