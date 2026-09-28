---
id: 1530
title: "The census recorder self-registered into every native C and C++ image,
  so a selector-less open was `Ambiguous` — and because issue 1050 gave the
  baked RMW rung to C++ and not to C, every native C example failed
  `nros_support_init` with `-3` for eighteen days"
status: resolved
type: bug
area: [rmw, api, testing]
severity: high
found: 2026-09-28
related: [1050, 1461, 0445, 0196, phase-463, phase-424]
---

## Symptom

Every native C example refuses to start, on all three backends, identically:

```
nros C Listener
===================
Locator:
Domain ID: 0
[nros] examples/native/c/listener/src/main.c:115 nros_support_init(&app.support, locator, domain_id) -> -3
```

`-3` is `NROS_RET_INVALID_ARGUMENT`. The C++ and Rust arms of the same suite
pass on the same build, in the same environment, in the same second.

This is what issue 1461's three C pubsub coordinates had been hiding. They had
produced no runtime result for eighteen days because a staleness verdict
absorbed them (issue 0445), so the failure above was never observed — the
coordinate reported a message about a `.stamp` file instead.

## Cause

Three facts, each correct on its own, which together refuse the open:

1. **`metadata-mode` is on for the NATIVE C++ umbrella by design** — phase-463
   W2, because the census binary IS the boot binary, so the recorder has to be
   in the one the host builds anyway. It is on the cargo line of every native
   C and C++ leaf:

   ```
   --features=ros-humble,rmw-cffi,std,platform-posix,metadata-mode,panic-platform
   ```

2. **`nros-rmw-metadata` self-registered** through a
   `nros_rmw_register_backend!` `.init_array` ctor, so the registry of every
   such image held two names — the shipping backend plus `metadata`.

3. **Since issue 1050 a selector-less open with more than one registered
   backend is a hard refusal.** `resolve_backend(None)` keys on the registry's
   LENGTH (`_ => BackendResolution::Ambiguous`), which `get_vtable` maps to
   `TransportError::InvalidConfig`, which `transport_error_to_ret` maps to
   `NROS_RET_INVALID_ARGUMENT`.

So the image is ambiguous, and the caller has to name a backend. **C++ does and
C cannot.** `NROS_ENTRY_RMW` is baked by `cmake/NanoRosEntry.cmake:855` for
every entry target regardless of language — it is on the C compile line:

```
DEFINES = -DNROS_ENTRY_RMW=\"cyclonedds\" -DNROS_HOST_POSIX ...
```

— and it has only two consumers, `nros-cpp/include/nros/executor.hpp:152` and
`node.hpp:2675`. The C surface reads it nowhere. `main.c` calls the three-argument
`nros_support_init`, whose `rmw` is `NULL`, so the image holds the answer in its
own preprocessor and never passes it.

**One commit introduced both halves.** `b64e3655a` (2026-09-05, issue 1050) is
where `get_vtable` began refusing `Ambiguous` and where the baked rung was
added — to cmake and to the C++ headers. The C road got the new failure mode and
not the new capability.

### The diagnosis was silent, and that is a fourth fact

The `Ambiguous` arm logs `"more than one RMW backend is registered and this open
named none; select one"` through `nros_log`. On a native host with no logger
installed that prints nothing, so the operator sees only `-3` — a code shared
with three other `TransportError` variants. Distinguishing `Ambiguous` from
`NoBackend` took `NROS_RMW=cyclonedds`, which succeeds: the registry was never
empty.

## The measurement that found it

```sh
env -u ROS_DOMAIN_ID -u NROS_LOCATOR -u NROS_RMW ./examples/native/c/listener/build-cyclonedds/c_listener
# -> nros_support_init ... -> -3
env NROS_RMW=cyclonedds ./examples/native/c/listener/build-cyclonedds/c_listener
# -> Support initialized ... runs
nm -C examples/native/c/listener/build-cyclonedds/c_listener | grep AUTO_REGISTER_CTOR
# -> nros_rmw_metadata::_::__NROS_RMW_BACKEND_AUTO_REGISTER_CTOR
```

## Fix

**The recorder's `.init_array` ctor is deleted.** It was never sufficient and it
was harmful wherever it did reach an image:

* *Never sufficient* — being an optional dep does not put the crate in the
  image, so rustc's staticlib DCE dropped the `#[no_mangle]` export and the ctor
  could not fire for code that was not there. `nros-cpp/src/rmw_backend.rs` says
  exactly this at its own explicit call ("the `.init_array` ctor cannot fire for
  code that is not in the image", phase-313). Every consumer registers by hand
  for that reason: `rmw_backend.rs`, `metadata_hooks.rs`, the census funnel in
  `nros-cpp/src/lib.rs`, and the generated entry TU
  (`entry.cpp.jinja`, asserted by `emit_cpp.rs`).
* *Harmful* — it is what made every native C and C++ image ambiguous.

The crate's own doc comment on `nros_rmw_metadata_register` predicted this
outcome: *"Registered by NAME, not as the default … Registering as default would
make the choice ambiguous and the executor would refuse to open at all."* The
ctor did register it as a competing name; only the reasoning about `default` was
about a distinction `BackendSlot` does not carry.

The one consumer that wants this backend SELECTED registers it and then names
it — `census_select_backend` sets `$NROS_RMW=metadata` — which is the whole
ladder rather than a ctor racing it.

## Verified

* The three C pubsub coordinates produce a runtime result for the first time in
  eighteen days, and it is **PASS**: `case_2_c_zenoh`, `case_5_c_cyclone`,
  `case_8_c_xrce`, 3 passed.
* A hand run with no selector reaches `Support initialized` and delivers.
* `AUTO_REGISTER_CTOR` is gone from the rebuilt binaries (count 0).
* The census/probe road still works: `nros-cli-core` 172 tests pass, including
  `metadata_mode_build_emits_source_metadata_for_component` and
  `metadata_build_discovers_missing_sources`.
* `nros-rmw-metadata`'s own 3 tests pass.

## Still open — the C surface has no baked rung

Deleting the ctor makes these images hold exactly one backend, so a nameless
open resolves and the symptom is gone. It does **not** give the C surface the
capability issue 1050 gave C++: a C image that legitimately links two backends
still cannot name one, because nothing on the C road reads `NROS_ENTRY_RMW`.
`nros_support_init_rmw` exists and takes the selector; what is missing is a
consumer of the bake, which for C has to be a header-side spelling so that the
twenty-odd in-tree C mains and every out-of-tree C app get it without changing a
line. Tracked as issue 1531.

## The gate for the class

The rule went into **`check-entry-rmw-vocabulary`** (fast line) rather than a new
gate, because that script already owns both vocabularies and the exemption list
that distinguishes them — `NOT_A_CMAKE_RMW = {"default", "metadata"}`, i.e. the
registry entries no `NANO_ROS_RMW` value can ever name. A second script would be
a second answer to "which names can an entry name", which is the drift this one
exists to prevent.

The third rule: **a name in `NOT_A_CMAKE_RMW` must not self-register through
`nros_rmw_register_backend!`.** The exemption says "no image selects this by
name"; a ctor puts it in the registry of every image that links the crate anyway,
and since issue 1050 that makes a selector-less open a hard refusal. A bakeable
name may self-register freely — that is how every real backend works.

**Its REACH was wrong on the first draft, and that is recorded because it is the
more useful half.** The gate reported `0 unbakeable name(s) present` over a tree
where `metadata` was registered AND self-registering: its `CALL_RE` read only
`nros_rmw_cffi_register_named(...)`, while this crate registers through the Rust
adapter's wrapper, `RustBackendAdapter::<MetadataRmw>::register_named(c"metadata")`.
So the exemption list had named `metadata` for a name the gate could not see, and
the new rule would have passed over the defect it was written for — issue 0196's
shape inside the fix for it. Both spellings are read now, with the adapter form
in the selftest, and the rule is verified against the real pre-fix tree rather
than only against synthetic input: with the ctor restored the gate FAILS naming
`packages/rmw/metadata/src/lib.rs`, and with it removed it passes.

One detail worth keeping: widening the regex to a bare `register_named`
alternative made it match the TAIL of `nros_rmw_cffi_register_named` in a
declaration and read that function's own `name` parameter as a backend name. The
existing declaration selftest caught it immediately, which is what that case was
for; `(?<![A-Za-z0-9_])` beside `(?<!fn )` is the fix.
