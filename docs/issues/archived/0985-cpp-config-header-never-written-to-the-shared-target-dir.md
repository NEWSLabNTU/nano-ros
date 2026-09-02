---
id: 985
title: "The configure-time heal wrote a museum sizes header over a correct one —
  and stamped it new, so ninja skipped the mirror that would have fixed it"
status: resolved
type: bug
area: cmake, build
severity: high
found: 2026-09-02
related: [issue-0978, issue-0805, issue-0268, issue-0834, issue-0369, issue-0088]
---

## Symptom

`just build native` fails linking `c_listener` (`fixture-linux-c-zenoh`) with
issue 0369's size anchor, carrying the SAME hash issue 0978 was filed for:

```
/usr/bin/ld: CMakeFiles/c_listener.dir/src/main.c.o:(.data.rel.ro+0x0):
  undefined reference to `nros_config_variant_sz_f3c40eb64e98fb7d'
```

**Not 0978 regressing** — 0978's fix is visibly working in the same
measurement, below.

## Measured

`examples/native/c/listener/build-zenoh`, four copies of
`nros_config_generated.h`, three anchors:

| copy | anchor | mtime |
| --- | --- | --- |
| `nros-c/include/nros/…` | `sz_9a3e918900c9d46d` | 2026-09-02 03:12:42 |
| `nros-cpp/include/nros/…` | `sz_f3c40eb64e98fb7d` | 2026-09-02 03:12:**43** |
| `nros-c/…` | `sz_cd6bc387c5d734f9` | 2026-09-01 04:02 |
| `nros-cpp/…` | `sz_f3c40eb64e98fb7d` | 2026-08-26 20:22 |

`libnros_cpp.a` — the mirror's own trigger — is **03:12:42**, one second BEFORE
the stale dest.

## The filing's hypothesis was WRONG, and the correction is the finding

It said the C++ side never writes `nros_config_generated.h` to the shared target
dir, so 0978's fallback arm was the only arm. That is false.
`nros-cpp/CMakeLists.txt` mirrors that header with gen-subdir
**`nros-c-generated`** — the same current source the C side uses. Running the
mirror command verbatim with the C++ leaf's own arguments settles it:

```
$ bash scripts/build/mirror-generated-header.sh \
    <leaf>/nano_ros/packages/api/nros-cpp/nros_config_generated.h \
    <leaf> nros-c-generated nros_config_generated.h /tmp/probe.h
$ grep -o 'sz_[0-9a-f]*' /tmp/probe.h
sz_9a3e918900c9d46d          # CURRENT
```

The mirror was never the writer of the stale copy.

## Root cause: a second writer, and it suppresses the first

`nros-cpp/CMakeLists.txt` carried a configure-time "heal" added by issue 0268 to
repair an already-drifted mirror. `nros-c/CMakeLists.txt` has **no** such block
— which is exactly why the two copies in one build dir disagree, and is the
controlled comparison that identifies it:

```cmake
foreach(_nros_cpp_hdr "nros_cpp_config_generated.h" "nros_config_generated.h")
    if(EXISTS "${CMAKE_CURRENT_BINARY_DIR}/${_nros_cpp_hdr}")
        file(COPY_FILE
            "${CMAKE_CURRENT_BINARY_DIR}/${_nros_cpp_hdr}"   # <- the leaf's OWN copy
            "${_nros_cpp_intree_include_dir}/nros/${_nros_cpp_hdr}"
            ONLY_IF_DIFFERENT)
    endif()
endforeach()
```

`${CMAKE_CURRENT_BINARY_DIR}/…` is the leaf's own corrosion output — precisely
the "present is not current" source issue 0978 removed from the mirror script,
for precisely the reason 0978 gives: once leaves share a cargo `--target-dir`,
the build script runs once per (crate, feature set) and every leaf after the
first keeps a copy that is present and arbitrarily old.

**And it makes the drift unrepairable.** The copy stamps the mirror's `OUTPUT`
with a new mtime, so ninja finds the output newer than
`$<TARGET_FILE:nros_cpp-static>` and skips the custom command that would have
written the current bytes. That is the 03:12:42 / 03:12:43 second in the table:
a repair for drift that causes drift and then suppresses its own fix.

## The open question is answered, not deferred

The filing said the fix hinged on whether a C target's include path *should*
reach `nros-cpp/include/nros/`. It does not hinge on that. The C++ package
mirrors the C sizes header **deliberately** — `nros-cpp/CMakeLists.txt` says so
("since the C++ umbrella bundles nros-c, Phase 241.D3-rev"), and both headers
are listed as outputs of the same custom command. So the C++ copy must be made
CURRENT, not unreachable, and no include-path change is involved.

## Not issue 0834

0834's signature is a `.stamp` with no `.h`, absorbing, repairable only by
`rm -rf`. Here nothing is absorbing: a re-configure repairs it in place, which
is what the verification below shows.

## Fix

The heal now runs the SAME resolver as the build-time mirror
(`mirror-generated-header.sh` via `execute_process`) instead of `file(COPY_FILE)`
from the leaf's binary dir. One writer, one precedence. It stays best-effort —
on a never-built tree neither candidate exists and the script exits non-zero
saying so, which is not a configure error because the build-time edge is the
real mechanism.

Verified incrementally, with **no wipe** (the anti-`rm -rf` rule — the point was
to find the edge, not to prove a clean build works):

```
BEFORE  nros-cpp/include/nros/nros_config_generated.h  sz_f3c40eb64e98fb7d
$ cmake examples/native/c/listener/build-zenoh     # re-configure, no wipe
AFTER   nros-cpp/include/nros/nros_config_generated.h  sz_9a3e918900c9d46d
$ ninja -C examples/native/c/listener/build-zenoh c_listener
[38/47] Linking C executable c_listener            # rc=0
```

## Gate

`just check config-header-single-writer` →
`scripts/check-config-header-single-writer.py`, fast line, static, no build.
Nothing may write a `*config_generated.h` except the mirror script.

**Its first version was vacuous and the record should say so.** It scanned for
the header name as a LITERAL, and 0985's actual code names it only through a
`foreach` variable — so the gate passed on the exact code it was written to
catch. It now resolves header-bearing variables, and the variable-shaped case is
one of its self-test cases so it cannot regress to literal-only. Against the
pre-fix tree:

```
check-config-header-single-writer: a SECOND writer of the per-build sizes header.
  packages/api/nros-cpp/CMakeLists.txt:164: file(COPY_FILE "${CMAKE_CURRENT_BINARY_DIR}/${_nros_cpp_hdr}" ...
```

## Acceptance

* [x] The C++ mirror copy is current, and a re-configure repairs an already
      drifted tree in place.
* [x] `ninja c_listener` links.
* [x] A gate that fails on the pre-fix tree, proven against the real code
      rather than a synthetic literal.
