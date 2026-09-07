---
id: 1132
title: "`cmake_language(DEFER ... CALL fn \"${local}\")` passes an EMPTY string,
  so the config-header ordering edges have never been applied"
status: resolved
type: bug
area: build, cmake
severity: medium
related: [issue-0088, issue-0090, issue-1084]
resolved_in: "phase-412 follow-up"
---

## What happens

`cmake_language(DEFER ... CALL <fn> <args>)` stores its arguments UNEXPANDED
and expands them when the deferred call runs, in the deferred directory's
scope. A function-local variable is gone by then, so the callee receives an
empty string.

Measured with a minimal project on cmake 3.22, which is this repo's floor:

```cmake
function(callee _arg)
    message(STATUS "DEFERRED CALLEE GOT: [${_arg}]")
endfunction()
function(caller _tgt)
    message(STATUS "CALLER SEES: [${_tgt}]")
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL callee "${_tgt}")
endfunction()
caller("my_target_name")
```

```
-- CALLER SEES: [my_target_name]
-- DEFERRED CALLEE GOT: []
```

## Where it bites

`cmake/NanoRosNodeRegister.cmake`:

```cmake
function(_nros_node_register_config_header_deps _tgt)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nros_node_register_apply_config_header_deps "${_tgt}")
endfunction()
```

`_nros_node_register_apply_config_header_deps` receives `""` and returns at its
own `if(NOT TARGET ...)` guard. Every `add_dependencies()` it exists to make --
the 0088/0090 config-header ordering -- has therefore never happened.

The damage is bounded and that is why it stayed hidden: the STRONGER edge
beside it, the `OBJECT_DEPENDS` file dependency, is applied EAGERLY and does
work. What is missing is the weaker target-ordering half, so the failure mode
is a rare ordering race rather than a wrong artifact -- which is exactly the
kind of thing that does not reproduce on the machine where you look for it.

## Why it was invisible

A no-op guarded by `if(NOT TARGET "")` is indistinguishable from a guard that
correctly skipped a non-target. Nothing asserts the dependency was added, and
the eager `OBJECT_DEPENDS` beside it made builds correct anyway. This is the
issue 0196 shape: a mechanism nobody can observe is indistinguishable from one
that was never written.

Found while restoring the declared-QoS producer (issue 1084), whose first draft
had the same bug for the same reason.

## The fix, and the pattern to prefer

Travel by a GLOBAL property and defer a NO-ARGUMENT call -- which is what
`_nros_node_register_schedule_inventory` already does, and what issue 1084's
producer now does:

```cmake
set_property(GLOBAL APPEND PROPERTY NROS_PENDING_X "${_tgt}")
cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _apply_pending_x)
```

Deliberately NOT fixed in the 1084 commit: re-enabling dormant build-ordering
edges for every image in this tree, inside a change about QoS depth, with no
image build to check it against, is the wrong trade. It wants its own commit
and a build.

## The sweep, done

Every `cmake_language(DEFER` in `cmake/` and `zephyr/`, checked rather than
left as an exercise. Only ONE site is affected:

| site | shape | verdict |
| --- | --- | --- |
| `NanoRosNodeRegister.cmake:202` | `CALL fn "${_tgt}"`, `_tgt` function-local | **BROKEN** -- the subject of this issue |
| `NanoRosNodeRegister.cmake:150` | no-argument call, state via GLOBAL property | safe |
| `NanoRosNodeRegister.cmake:1289` | no-argument call (issue 1084's producer) | safe |
| `NanoRosEntityFacts.cmake:144` | no-argument call, GLOBAL property | safe |
| `NanoRosEntry.cmake:104` | `cmake_language(EVAL CODE ...)` with `[[${target}]]` | safe -- EVAL expands it into a LITERAL before DEFER stores it, which is the other correct workaround |

So the class has exactly one member, and two working idioms already exist in
the tree to fix it with. The `EVAL CODE` form at `NanoRosEntry.cmake:104` is
worth knowing about: it is the only way to defer a call that genuinely needs an
argument, and it works because the expansion happens at EVAL time rather than
at execution time.

## Fixed, and MEASURED on the island image

The one broken site now appends its target to a GLOBAL property and defers a
NO-ARGUMENT drain, which is the idiom
`_nros_node_register_schedule_inventory` already used two functions above. The
list form is needed because several callers register different targets before
the deferred call runs, and the drain is scheduled once rather than per call.

Measured by putting a `message(STATUS ...)` at the top of the callee and
re-running cmake configure on the safety-island Zephyr image
(`mr_canhubk3/s32k344`), once with each form. Same build directory, same cache,
nothing else changed:

```
BEFORE (the shipped form, DEFER ... CALL fn "${_tgt}")
  5 calls, and every one of them:
  -- NROS1132PROBE applying to []

AFTER (GLOBAL property + no-argument DEFER)
  5 calls:
  -- NROS1132PROBE applying to [mrm_comfortable_stop_operator_lib]
  -- NROS1132PROBE applying to [mrm_emergency_stop_operator_lib]
  -- NROS1132PROBE applying to [mrm_handler_lib]
  -- NROS1132PROBE applying to [stop_mode_operator_lib]
  -- NROS1132PROBE applying to [zephyr_entry]
```

So the callee ran five times per configure for the whole life of this code and
did nothing five times, exactly as this issue predicted.

## What the fix does NOT change, on this image

Every one of those five targets ALREADY carried the
`nros_c_cargo_build` / `nros_cpp_cargo_build` order-only edge in `build.ninja`
before the fix -- checked on a build dir produced by the broken code. The eager
`OBJECT_DEPENDS` half was supplying it, which is what this issue says and is why
nothing ever failed. The fix restores the weaker target-level half for the cases
that half does not cover; on THIS image it is redundant, and no before/after
difference in the generated ninja is claimed.

## The gate

`scripts/check-deferred-call-args.py`, in the `ci` group. It parses cmake
invocations (quoted strings and bracket arguments as single tokens) and fails on
any `cmake_language(DEFER ... CALL fn <arg>)` whose argument carries a `${`.
Both correct idioms pass: the no-argument form, and the
`cmake_language(EVAL CODE ...)` wrapper, whose DEFER lives inside a quoted
string and is therefore not a token of the statement the gate reads.

7 self-test cases, including the false reading the gate shipped with and had to
fix: it flagged the cmake COMMENT that quotes the broken form in order to
explain it. A scanner that cannot tell code from prose about code will keep
finding itself.

## Not verified: a linked image

The island image does not link on nano-ros main right now -- `region DTCM
overflowed by 45040 bytes` -- and that is NOT this change. A control build of
clean main with this change stashed produces the identical overflow, to the
byte. Filed separately. The configure-level measurement above is what backs
this fix.
