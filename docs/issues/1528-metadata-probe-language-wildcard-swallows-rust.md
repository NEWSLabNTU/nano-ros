---
id: 1528
title: "The cmake metadata probe picks its language with `_ => \"cpp\"`, so the
  only thing keeping a Rust component out of a C++ probe is a guard 170 lines
  away in another function"
status: open
type: bug
area: cli, codegen, metadata
severity: low
found: 2026-09-28
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

## Enforcement

The measurement, not the assertion: add a throwaway `Language` variant, run
`cargo check --workspace --all-targets --keep-going` in `packages/cli`, and
confirm this site appears among the errors afterwards and does not before.
