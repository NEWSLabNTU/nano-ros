---
id: 1673
title: "Every Zephyr C + Cyclone DDS image fails to link on a duplicate topic
  descriptor — two roads generated the same descriptors, and a global
  `--allow-multiple-definition` had hidden it until issue 1636 scoped the flag"
status: resolved
type: bug
area: [zephyr, rmw, build]
severity: high
found: 2026-10-03
related: [1636, 1645, 1674, 0155, phase-347]
---

## Symptom

Found by `just build-test-fixtures lane=all` once issue 1672 let it past the
Zephyr Rust leaves. Exactly six fixtures fail, all Zephyr **C + Cyclone DDS** —
talker, listener, both service roles, both action roles:

```
ld: libstd_msgs__cyclonedds_ts.a(String_register_0.c.obj): multiple definition of
    `register_std_msgs_String_0'; app/libapp.a(String_register_0.c.obj): first defined here
```

and for the action images, every `action_msgs` / `builtin_interfaces` /
`example_interfaces` descriptor the same way.

## Cause — two roads, one mask, and the mask was removed

**Two roads generate the same Cyclone descriptors into one image.**

* The automatic road: `zephyr/cmake/nros_generate_interfaces.cmake` builds
  `<pkg>__cyclonedds_ts` for every `find_package`'d interface package and links
  it `--whole-archive` into `app` (phase-347 W5, 2026-08-11).
* A hand road in each example: a block calling
  `nros_rmw_cyclonedds_generate_from_msg`, or the action helper
  `nros_zephyr_add_cyclonedds_action_descriptors`, compiling the same TUs into
  `app` — which Zephyr also whole-archives.

Both define the same strong symbols. A global `--allow-multiple-definition` kept
the first and hid the rest.

**Issue 1636 (`f513b9cdcd`, 2026-10-02 23:51) scoped that flag** to images linking
a C++ message-FFI staticlib, and its comment records that a C image "links with
the flag absent" — measured on `c/talker`, evidently with zenoh, not Cyclone.
That was correct about the flag and unmasked this.

**The C++ images were not passing — they were masked.** Their link line still
carries the flag (for 1645's FFI duplicates), so the same duplicate descriptors
linked by silently keeping the first copy. Same build structure as C, measured:
type-support library whole-archived, hand TU compiled into `app`, the symbol
defined in both archives.

**The trap that decided the fix.** The automatic road was guarded on
`COMMAND nros_rmw_cyclonedds_generate_from_msg` — a command that exists only once
`NrosRmwCycloneddsTypeSupport` is included, and the only thing that included it
was the example's own hand block (for the action examples, *inside* the helper's
function body). So deleting the hand blocks alone would have turned a link error
into a SILENT skip: no descriptors, no error, `find_descriptor()` failing at
runtime. The guard failed open the other way too: an image that `find_package`d
an interface package without hand-including the module got no descriptors at all.

## Fix

* **The automatic road arms itself.** `nros_generate_interfaces.cmake` includes
  the module by FULL PATH from `NROS_CYCLONE_CMAKE_DIR` (a cache variable the
  Cyclone backend exports, so no `CMAKE_MODULE_PATH` scoping is involved), sets
  `IDLC_EXECUTABLE` and the scripts dir itself, and makes an absent module a
  configure error instead of a skip.
* **The hand road is retired from the twelve C and C++ examples** that also
  `find_package` the same package. Their comments claimed the codegen "emits the
  rcl-style C message struct but not Cyclone's descriptor", which stopped being
  true at phase-347 W5. **The six Rust examples keep theirs**: they do not
  `find_package` the interface packages, so for them the hand road is the only
  one.

## Verified

**Removal loses nothing — a strict superset, measured before deleting.** Every
global symbol the hand road defined (2, 2, 4, 4, 30 and 30 across the six C
images) is also defined by the generated type-support libraries; the same holds
for C++. (The first version of that check was vacuous — zsh does not word-split
an unquoted variable, so it scanned zero objects and reported zero missing. It
was redone under bash with the object and symbol counts printed.)

**Builds — all fifteen affected images, each relinked by the run:**

```
ok fresh=yes dups=0   c-{talker,listener,service-client,service-server,action-client,action-server}
ok fresh=yes dups=0   cpp-{talker,listener,service-client,service-server,action-client,action-server}
ok fresh=yes dups=0   rust-talker, rust-action-server, ws-cpp-entry       (controls)
```

**Descriptors are present, once each** — read off `zephyr.exe` (on native_sim
`zephyr.elf` is a relocatable intermediate, which a first check read by mistake
and found empty): the key descriptor exactly once in all twelve images, 33
descriptors for the `std_msgs` images and 51 for the `example_interfaces` ones,
with one register function per descriptor.

**Boots:** all eight Cyclone C/C++ `boot_smoke` cells pass.

**Runtime delivery is a separate, pre-existing defect — issue 1674.** Every
Cyclone native_sim e2e cell fails with `received 0 sample(s)`, C and C++ alike,
and an A/B rebuilding the C++ pair from `origin/main`'s own sources fails
identically. This change neither causes nor fixes it.
