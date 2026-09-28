---
id: 1528
title: "The cmake metadata probe picks its language with `_ => \"cpp\"`, so the
  only thing keeping a Rust component out of a C++ probe is a guard 170 lines
  away in another function"
status: resolved
type: bug
area: cli, codegen, metadata
severity: low
found: 2026-09-28
resolved_in: "431d0f206 (phase-469 audit S2 follow-on)"
related: [phase-469, issue-1469, issue-1470, issue-1062]
---

## What happens

`orchestration/metadata_refresh.rs::cpp_probe_options` decides which language
the cmake metadata probe is generated for:

```rust
let language = match decl.config.language {
    ComponentLanguage::C => "c",
    _ => "cpp",
};
```

`ComponentLanguage` is `nros_lang::Language` — `Rust | C | Cpp`. So the
wildcard's coverage set is `{Cpp, Rust}`, and the string it hands the probe
(`CmakeProbeOptions.language`, consumed by `metadata_probe_cmake` to pick the
probe TU's extension and the ABI seam it links against) is `"cpp"` for a Rust
component as readily as for a C++ one.

This is the **third** site of the shape the 2026-09-27 codegen audit measured
and the audit missed it. The two it named — `cmd/codegen.rs`'s
`is_cpp = lang != "c"` and `orchestration/workspace.rs`'s `_ => "cpp"` — landed
as phase-469 S2 (`85bb4d62c`). That commit's own sweep found this one and
**deliberately left it**, reporting rather than editing it, on the grounds that

> It cannot be mechanically closed, because `Rust` reaches the wildcard today
> and what a Rust component should send a C/C++ probe is its own question.

## What I measured

### Can a Rust component actually reach it? No — and that is the point

`cpp_probe_options` is module-private and has **exactly one call site**:

```
$ git grep -n cpp_probe_options -- '*.rs'
packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs:112:  match cpp_probe_options(decl, nano_ros, &probe_root) {
packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs:253:fn cpp_probe_options(
```

and line 112 sits inside

```rust
if decl.config.language != ComponentLanguage::Rust {
    // phase-313 — C/C++ probes are COLLECTED here and run as one batch below
    match cpp_probe_options(decl, nano_ros, &probe_root) { … }
    continue;
}
```

A Rust declaration takes the branch below it — `build_metadata`, the cargo
harness — and never reaches the match. **So the S2 report's premise is not
true of the call path**: `Rust` is in the wildcard's *type-level* coverage, not
in its *reachable* set. Today's routing is correct; nothing is mis-probed.

Census over the tree, to check that the guard is load-bearing rather than
vacuous — a throwaway integration test walking every `package.xml` under
`examples/` and asking `Workspace::component_declarations()` for each, then
bucketing by `decl.config.language` and by the routing predicate above:

```
packages walked: 292
declarations:    152
by language:     {"C": 34, "Cpp": 39, "Rust": 79}
reaching cpp_probe_options (language != Rust): {"C": 34, "Cpp": 39}
```

79 Rust declarations exist and 0 of them reach the probe; 73 C/C++ ones do, and
they are the whole reachable set. (`ComponentLanguage::Rust` is also the
DEFAULT for a `[[component]]` row that names no language —
`workspace.rs:429` — so the common way to arrive at `Rust` is silence, which
makes the guard the thing standing between an under-specified manifest and a
C++ probe.)

### So what is the defect

The probe's C-vs-C++ decision is **correct only because of a predicate in a
different function**, and the compiler cannot see the relationship. Two
consequences, both of which the two landed siblings were fixed for:

1. **A fourth `Language` variant is routed silently.** The caller's guard is
   `!= Rust`, so anything new passes it; the callee's wildcard then calls it
   C++. No compile error, no diagnostic, no `report.unsupported` row. The
   resulting probe TU is `.cpp`, is generated against the C++ component-object
   shape, and links against the C++ ABI seam.
2. **Where that failure lands is already on record.** Issue 1062 is this exact
   consequence from the other direction — a C++ component probed as C — and
   what it looks like is a link error two layers down, in generated code, with
   no path back to the decision:

   ```
   probe_controller_pkg__controller.cpp:(.text+0x6a): undefined reference to
         `__nros_c_component_controller_pkg_create'
   ```

   Every one of these surfaces to the user as `sync: source metadata — no
   producer for <pkg>::<comp>`, the same undifferentiated line issues 1469 and
   1470 were filed behind.

The wildcard is therefore unreachable-but-wrong: it costs nothing today and it
is the one construct in this function that a new language can walk through
unannounced.

## Fix direction

Make the match exhaustive with no wildcard, the shape the two landed siblings
use. `Rust` gets a **refusal that names the component and says why**, not a
silent fallback and not a bare skip: `cpp_probe_options` already returns
`Result<_, String>` whose `Err` is recorded as `pkg::comp (why)` in
`RefreshReport::unsupported` and printed by `nros sync`, which is exactly the
"a probe outcome carries its cause" property issue 1469 landed. A Rust
component reaching the cmake probe is a routing bug, so the message should say
that rather than pretend it is a user's mistake.

Because `Rust` is unreachable, the refusal changes no behaviour for any
component in the tree; what it buys is that the caller's binary predicate and
the callee's decision become one construct the compiler can check, so a fourth
variant is a build error here instead of a `.cpp` probe nobody asked for.

## Resolution — `431d0f206`

The match is exhaustive, with no wildcard:

```rust
let language = match decl.config.language {
    ComponentLanguage::C | ComponentLanguage::Cpp => decl.config.language.as_str(),
    ComponentLanguage::Rust => return Err("a Rust component is produced by the cargo \
        metadata harness, not by the cmake C/C++ probe — reaching the probe is a routing \
        bug, since the caller's `language != Rust` guard should have sent it to \
        `build_metadata`".to_string()),
};
```

Three choices worth their reasons:

* **A refusal, not a skip and not a fallback.** `cpp_probe_options`'s `Err`
  is already recorded as `pkg::comp (why)` in `RefreshReport::unsupported`
  and printed by `nros sync`, so the refusal arrives with its cause — the
  property issue 1469 landed one file over. A silent fallback was the one
  answer not available; a bare skip would have been an outcome with no
  reason attached, which is the shape 1469 and 1470 were both filed behind.
* **The message says where a Rust component IS produced** (`build_metadata`)
  and that arriving here is a routing bug, because `Rust` is unreachable by
  construction — nobody reading this line has done anything wrong except
  write a router.
* **`C | Cpp` yield `Language::as_str()`** rather than re-spelling `"c"` and
  `"cpp"`. Identical output, one producer — phase-469 S2's move in
  `workspace.rs`.

### Negative direction

`the_probe_language_is_decided_with_no_wildcard` (unit, in this module):
C still probes as `c`, C++ as `cpp`, and Rust refuses with a message naming
the harness. Mutating the live arm to `"c"` fails the test, so it is not
vacuous. `cargo test -p nros-cli-core --lib`: 1446 passed.

### Enforcement, measured

A throwaway `Language::Zig` — plus the `ALL`, `as_str` and `of_sources` arms
`nros-lang` needs in order to compile at all — then
`cargo check --workspace --all-targets --keep-going` in `packages/cli`:

| | sites named |
| --- | --- |
| before | `cmd/codegen.rs` 451, 528, 639; `codegen/entry/pack.rs` 141; `orchestration/workspace.rs` 1496 |
| after | the same five, **plus `orchestration/metadata_refresh.rs` 296** |

The variant was reverted; `nros-lang` is untouched by the fix.
