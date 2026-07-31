# Phase 325 — uORB interop: direct, and bridged to any RMW

**Status (2026-07-31): Draft.** Not started.
**Implements:** RFC-0026 (example layout), RFC-0048 (cmake consumption).
**Successor to:** [phase-316](phase-316-example-tree-axes.md) W4, which carried
the decisions but not the work — scoping showed W4 is a phase, not a work item.
**Informed by:** issues 0351 (proofs that observe the wrong thing), 0356
(`px4_e2e` targets a retired tree), 0288, 0159 (`.clang-format-ignore` precedent).

## Goal

Two demonstrations, making two distinct claims:

| | claim | proven against | falsified by |
| --- | --- | --- | --- |
| **direct** (W2) | nano-ros speaks PX4's in-memory format, so no serialization happens at all | a **stock, unmodified PX4 module** | a stock module cannot read the topic |
| **bridge** (W3) | nano-ros carries uORB traffic out to any RMW it supports, selected at build time | a **real ROS 2 node** | ROS 2 cannot see it, or only one backend works |

Both acceptances name a **foreign peer**, and that is not incidental. A
nano-ros↔nano-ros test passes identically whether the encoding is right or
wrong, because both ends share the bug — issue 0351's shape, hit twice during
phase-316. The stock PX4 module and the real ROS 2 node ARE the measurement; a
demo that drops them proves nothing it claims to prove.

## Why uORB is the special one

Decided by the maintainer, and load-bearing in the code rather than aspirational:

| | every other backend | uORB |
| --- | --- | --- |
| wire bytes | CDR encoding of the message | the PX4 C struct, verbatim |
| type identity | ROS type name + type hash | `ORB_ID(<topic>)`, a static descriptor |
| serialization cost | encode + decode per sample | none — the payload IS the struct |
| who can read it | another nano-ros / ROS 2 endpoint | **any stock PX4 module**, unmodified |

`publisher_publish_raw` checks `len >= meta->o_size` and hands the caller's bytes
straight to `orb_publish`. `publisher_create` ignores `type_name`, `type_hash`,
`qos` and `domain_id`, resolving the topic through `nros_rmw_uorb_register_topic`
to a `const struct orb_metadata *`. Everywhere else nano-ros interoperates by
speaking a wire protocol; here it interoperates by **sharing PX4's in-memory
type**.

That is also the cleanest statement of why `examples/px4/cpp/uorb/` looked like an
RMW path level and was not one (phase-316): uORB is not a transport choice, it is
the absence of a transport.

## PX4 convention is normative for everything inside a PX4 module

**Maintainer instruction (2026-07-31): the example structure and content follow
PX4 convention.** Not nano-ros's house style, and not a hybrid. A PX4 module is
read, reviewed and maintained by PX4 people; it should look like the modules
next to it in `src/examples/`.

Verified against the vendored tree (`third-party/px4/PX4-Autopilot`) rather than
recalled. The reference is `src/examples/work_item/` — the canonical C++ module.

### Layout

```
<EXTERNAL_MODULES_LOCATION>/src/modules/<snake_name>/
    CMakeLists.txt      BSD 3-clause header, then px4_add_module(...)
    Kconfig             menuconfig <SECTION>_<NAME>, bool, default n, ---help---
    <CamelCase>.hpp     class decl, matching the class name
    <CamelCase>.cpp     impl + the extern "C" entry point
```

C++ modules name files after the CLASS (`WorkItemExample.cpp/.hpp`), not after
the module (`work_item`). Plain-C modules use `snake_name.c`
(`px4_simple_app.c`). Directory is snake_case; class is CamelCase; the
`px4_add_module` `MODULE` argument is `<section>__<name>`
(`examples__work_item`).

**This does not conflict with RFC-0026.** The path rule phase-316 enforced is
about the `<plat>/<lang>/<case>` LEVELS; what an example contains internally is
its own business. So `examples/px4/cpp/firmware/` is the case dir, and inside it
sits the PX4-required `src/modules/<name>/` tree — the example dir IS an
`EXTERNAL_MODULES_LOCATION` root. Note this is the same collision that produced
the hoist+shim phase-316 W3.1 deleted; it is fine here only because the example
dir is the root, not a leaf inside one.

### The module class

```cpp
class NrosUorbTalker : public ModuleBase<NrosUorbTalker>, public ModuleParams,
                       public px4::ScheduledWorkItem
```

`ModuleBase<T>` is what gives a module `start` / `stop` / `status` from the pxh
shell for free. Required members:

| member | why |
| --- | --- |
| `static int task_spawn(int argc, char *argv[])` | ModuleBase contract |
| `static int custom_command(int argc, char *argv[])` | ModuleBase contract |
| `static int print_usage(const char *reason = nullptr)` | ModuleBase contract |
| `int print_status() override` | what `<module> status` prints |
| `void Run() override` | the work-queue tick |
| `bool init()` | schedules the first run / registers callbacks |

Entry point, at the bottom of the `.cpp`:

```cpp
extern "C" __EXPORT int nros_uorb_talker_main(int argc, char *argv[])
{
	return NrosUorbTalker::main(argc, argv);
}
```

`MAIN` in `px4_add_module()` must match the `<name>_main` symbol.

### Usage strings are not optional

```cpp
PRINT_MODULE_DESCRIPTION(R"DESCR_STR(
### Description
...
)DESCR_STR");
PRINT_MODULE_USAGE_NAME("nros_uorb_talker", "examples");
PRINT_MODULE_USAGE_COMMAND("start");
PRINT_MODULE_USAGE_DEFAULT_COMMANDS();
```

This is what `<module> help` prints AND what PX4 scrapes for its module
reference docs. A module without it is invisible in PX4's documentation.

### Kconfig

Every module dir carries one:

```
menuconfig EXAMPLES_NROS_UORB_TALKER
	bool "nros_uorb_talker"
	default n
	---help---
		Enable support for nros_uorb_talker
```

`integrations/px4/sitl-overlay/render-overlay.sh` already walks
`<px4>/src/modules/nros_*/` and renders the defconfig fragment — reuse it, do not
add a second mechanism.

### Style: PX4's, and it conflicts with ours

| | PX4 | nano-ros |
| --- | --- | --- |
| indent | **tab**, `tab_width = 8` | 4 spaces |
| max line | 120 | 100 |
| formatter | `Tools/astyle/fix_code_style.sh` | `.clang-format` (LLVM-based) |
| file header | **BSD 3-clause block, every file** | none required |

These are not reconcilable. **Correction (2026-07-31): our formatter is not the
cause.** I assumed `check-c-fmt` / `check-cpp-fmt` were reformatting PX4 modules;
they are not. Both enumerate explicit paths — `nros-c/include`, `zpico-zephyr`,
`examples/native/c/**`, `nros-cpp/include/nros/*.hpp` — and no PX4 module tree is
among them. The 4-space style was simply how the file was written. Checking
before acting is the difference between a guard and a no-op dressed as a fix.

What was true regardless:
`packages/testing/nros-px4-register-check/.../nros_register_check.cpp` is
**4-space indented, carries no BSD header, and is a bare `extern "C"` main** with
no `ModuleBase`, no `print_usage` and therefore no `status` / `stop` / `help`. It
is a nano-ros file wearing a PX4 file's location.

- [x] **W0.1** `.clang-format-ignore` gains the PX4 module trees. A **guard, not a
      fix** — nothing formats them today. But both fmt recipes enumerate paths by
      hand, so the day someone widens a glob to `examples/**/*.cpp` (a likely and
      otherwise-correct change), PX4 trees would be silently reflowed into
      4-space. `.clang-format-ignore` is read by clang-format itself, so the guard
      holds whatever the recipe globs become. Precedent: `cmake/templates/*`
      (issue 0159 — reflow broke `@VAR@` tokens).
- [x] **W0.2** `nros-px4-register-check` brought to PX4 convention.

**W0 lands before W2**, so the first real example is written in the right style
rather than converted afterwards.

### One thing that is NOT PX4 convention, deliberately

A PX4 module normally reaches uORB through `uORB::Publication<T>` /
`uORB::Subscription`. **The demo must not** — those bypass nano-ros entirely, so
a module using them proves nothing about nano-ros and would pass identically if
the backend were deleted. The demo publishes through the **nano-ros** publisher
(`publish_raw` over the `<uORB/topics/*.h>` struct); everything AROUND that call
— module class, Kconfig, usage strings, style, file naming — is PX4's.

Same family of trap as the foreign-peer rule above: a proof that observes
something common to the working and broken cases proves nothing.

## What is already true

Worth stating precisely, because three artifacts look like PX4 integration and
the tree reads as though this is solved:

| artifact | what it actually exercises |
| --- | --- |
| `nros-rmw-uorb/tests/register_smoke.cpp` | drives the RMW **vtable directly**, stubbing `nros_rmw_cffi_register` AND the uORB ABI. Never touches `nros-cpp`. |
| `packages/testing/nros-px4-register-check/` | compiles the backend inline against **real PX4 headers** and calls `nros_rmw_uorb_register()`. Proves it LINKS. Does not link `nros-cpp` — the weak `register_fallback.c` exists precisely so it need not. |
| `integrations/px4/module-template/nano_ros_app.cpp` | the node code is a **comment**: *"Replace this comment block with NodeBuilder / Publisher calls"*. |

So: **no nano-ros node has ever been constructed on the uORB backend.** The
backend's proven surface stops below the node API. `examples/README.md` called the
register-check "the canonical PX4 uORB surface" — true about linking, easy to
misread as usage.

Two things that ARE proven and remove risk:

- **`publish_raw` / `subscription_take` are already public** on both the C and C++
  APIs. The direct example needs no new data-plane machinery.
- **Two live backends in one image works.** `examples/bridges/tt-zenoh-to-cyclonedds`
  does `nros_rmw_zenoh::register()` + `nros_rmw_cyclonedds_sys::register()` then
  `Executor::open_with_rmw("zenoh", &cfg)` and opens a second session.
  `open_with_rmw` takes the backend by **name**, so build-time selection needs
  only a cargo feature choosing which `register()` compiles in and which name
  string is passed.

## The actual gap: consumption, not a platform port

phase-316's note said "there is no `cmake/platform/nano-ros-px4.cmake`, and every
other platform has one". That is true and **the wrong diagnosis** — recorded here
because a wrong diagnosis points at the wrong fix, which is this session's
recurring lesson.

Platform modules are consumed by nano-ros's OWN root `CMakeLists.txt`
(`cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake`, resolved at
`CMakeLists.txt:116`). A PX4 module is built by **PX4's** cmake via
`px4_add_module()` and never enters that file. And SITL is an ordinary host
x86_64 process, so the platform shim it needs is `posix`, which already exists.

The gap is a **consumption path**: how a `px4_add_module()` target links
`libnros_cpp.a` + the posix platform shim + the uORB backend.

~~That is RFC-0048 territory — `find_package(nano_ros)` → `_nros_bootstrap` →
`add_subdirectory`.~~ **Wrong, measured in W1.1 (see below).** `_nros_bootstrap`
works by `add_subdirectory`, which compiles nano-ros sources inside PX4's cmake
under PX4's `-Werror -Wfatal-errors -Wpedantic …` flags, and the posix shim does
not survive them. The working shape is the opposite: **nano-ros builds its own
artifacts, PX4 links prebuilt archives.**

**Real PX4 boards (NuttX, cross-compiled) are explicitly out of scope.** Both
demos run on SITL. A board port is the `nuttx` platform plus a cross toolchain and
is its own phase; nothing here should pretend to deliver it.

## Work items

### W0 — PX4 convention, before anything is written in it

Both items are stated in full under "Style: PX4's, and it conflicts with ours"
above; listed here so the sequencing is unmissable.

- [ ] **W0.1** `.clang-format-ignore` the PX4 module trees; PX4's astyle owns them.
- [ ] **W0.2** Bring `nros-px4-register-check` into PX4 convention (BSD header,
      tabs, `ModuleBase` + `print_usage`), or record why not.

**Acceptance:** `nros_register_check help` prints its `PRINT_MODULE_DESCRIPTION`
from the pxh shell. **DONE 2026-07-31** — verified live:

```
### Description
Link/registration check for the nano-ros uORB RMW backend.
Usage: nros_register_check [arguments...]
INFO  [nros_register_check] nros_rmw_uorb_register() -> OK
```

Note the acceptance says `help`, not `status`. An earlier draft said `status` —
wrong: `status` comes from `ModuleBase<T>`, which is for modules that DAEMONIZE.
This one is a one-shot command, modelled on `src/systemcmds/gpio`, so it has no
start/stop/status to offer and inheriting `ModuleBase` to fake them would be
worse than offering nothing. The old header even documented
`nros_register_check start` — an invocation that never existed.

### W1 — a PX4 module can consume nano-ros

- [x] **W1.1** Prove a `px4_add_module()` target can link nano-ros. **DONE** —
      see the result below. The friction predicted here ("PX4's module factory and
      nano-ros's `add_subdirectory` import disagree about flags") is exactly what
      happened, and it disqualifies `find_package(nano_ros)` rather than needing a
      workaround.
- [ ] **W1.2** Wrap the result as ONE helper — working name
      `nros_px4_add_module()` — under `integrations/px4/`, so a module author
      writes one call. Not a copy of `px4_add_module`'s argument surface: forward
      to it. Per W1.1 it is a **link helper, not a bootstrap**: two prebuilt `.a`
      paths plus the registration hook, and it must not call
      `find_package(nano_ros)`.
- [ ] **W1.3** Retire the module-template's comment-block placeholder in favour of
      the helper, so the template compiles what it documents. A template whose
      body is `// Replace this comment block` is how the gap stayed invisible.

**Acceptance:** a PX4 SITL build produces a module that links `libnros_cpp.a` and
starts. No node behaviour yet — that is W2. **MET for W1.1.**

#### W1.1 RESULT (2026-07-31): it links, and it runs

**Answered: yes.** A `px4_add_module()` target links `libnros_cpp.a` and starts.
Receipt, from the pxh shell after a full SITL build (`rc=0`, zero undefined
references):

```
INFO  [nros_link_check] nros-cpp linked: nros_rmw_cffi_register=0x5fdc2807ab1d nros_cpp_node_create=0x5fdc28051638
```

The probe takes the ADDRESS of each symbol rather than calling it: the question
was whether they resolve at link time, and printing a real address proves that
without needing an initialised runtime. Calling them would have conflated "does
it link" with "does it work", which is the distinction W1 exists to settle.

##### Three link inputs, not one

`libnros_cpp.a` alone leaves exactly 10 undefined symbols, in two families —
both documented, neither surprising:

| missing | supplied by |
| --- | --- |
| `nros_platform_{wake_init,wake_wait_ms,wake_signal,wake_drop,wake_storage_size,wake_storage_align,sleep_ms}` | the **posix** platform shim (`packages/core/nros-platform-posix`) |
| `nros_app_register_backends` | normally a strong-stub TU **generated** by `nano_ros_link_rmw()` (`cmake/NanoRosLink.cmake`); a hand-rolled PX4 module gets no generation and must define it |

The platform half confirms the phase's premise: SITL is an ordinary host process,
so it wants the **same posix shim every native build uses**. No platform port.

##### CORRECTION: `find_package(nano_ros)` is the wrong consumption path

This phase said the gap was "RFC-0048 territory —
`find_package(nano_ros)` → `_nros_bootstrap` → `add_subdirectory`". **Measured,
that is wrong**, and it is the third correction to my own analysis in this phase.

`_nros_bootstrap` works by `add_subdirectory`, which builds nano-ros sources
*inside PX4's cmake* — where they inherit PX4's flags:

```
-Werror -Wfatal-errors -Wpedantic -Wnested-externs -Wbad-function-cast
-Wshadow -Wdouble-promotion -Wfloat-equal -Wlogical-op ...
```

That set is far stricter than nano-ros's own, and `nros-platform-posix` does not
survive it — every TU died on `"_DEFAULT_SOURCE" redefined [-Werror]`, PX4 having
already defined it. Fixing that one macro would only buy the next warning.

**The shape that works: nano-ros artifacts are built by nano-ros's build, and PX4
links prebuilt archives.** Each project keeps its own warning policy on its own
sources. This is already how `libnros_cpp.a` reaches the link (cargo builds it;
cmake only links it) — the platform shim simply has to follow the same rule
instead of being pulled into PX4's tree:

```sh
cargo build -p nros-cpp --no-default-features --features std,rmw-cffi --release
cmake -S packages/core/nros-platform-posix -B <dir> && cmake --build <dir>
```

So W1.2's helper is a **link helper, not a bootstrap**: it points a PX4 module at
two prebuilt `.a` files and provides the registration hook. It must NOT call
`find_package(nano_ros)`.

##### Reproducing the probe

`EXTRA_CMAKE_ARGS` is not forwarded by PX4's Makefile — pass configuration
through the environment, as `NROS_REPO_DIR` already is:

```sh
export NROS_PLATFORM_POSIX_A=<dir>/libnros_platform_posix.a
make -C third-party/px4/PX4-Autopilot px4_sitl_default \
     EXTERNAL_MODULES_LOCATION=<probe-root>
```

##### Toolchain gotcha worth knowing

The system `nm` cannot read these archives — `LLVM gold plugin has failed to
create LTO module: Opaque pointers are only supported in -opaque-pointers mode
(Producer: LLVM22.1.2-rust-1.96.0 Reader: LLVM 14.0.0)` — and reports **no
symbols**, which reads exactly like an empty archive. Use the toolchain's own:

```sh
~/.rustup/toolchains/<tc>/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-nm
```

`libnros_cpp.a` exports 124 `nros_cpp_*` symbols; a bare `nm` says 0.


### W2 — the direct demo: nano-ros ↔ a stock PX4 module

- [ ] **W2.1** A nano-ros node inside a PX4 module that publishes a real PX4
      topic:
      `nros_rmw_uorb_register_topic("/<topic>", "<ros_type_name>", ORB_ID(<topic>))`,
      then `publish_raw((const uint8_t *)&msg, sizeof msg)` with `msg` a
      `<uORB/topics/*.h>` struct. The message type comes from PX4's headers, NOT
      from `nros generate-*`.
- [ ] **W2.2** The subscribe direction, reading a topic a stock PX4 module
      publishes.
- [ ] **W2.3** Lands at `examples/px4/cpp/firmware/` — which this creates.
      phase-316 W3.1 deliberately left the dir uncreated rather than empty. That
      dir is an `EXTERNAL_MODULES_LOCATION` root, so the module sits at
      `firmware/src/modules/<snake_name>/` per PX4's requirement.
- [ ] **W2.5** Written to PX4 convention throughout — `ModuleBase<T>`, `Kconfig`,
      `PRINT_MODULE_*` usage strings, CamelCase files matching the class, tabs,
      BSD header. See the normative section above. The only deliberate departure
      is that publishing goes through the **nano-ros** publisher rather than
      `uORB::Publication<T>`.
- [ ] **W2.4** A test that observes the exchange **from the PX4 side**: `listener
      <topic>` in the SITL shell, or an upstream module that already subscribes
      it. Assert on that output.

      The harness exists — reuse it, do not write a second one. `Px4Sitl` (from
      the `px4-sitl-tests` path-dep in `nros-px4-sitl-test`) gives boot, pxh
      shell, log-wait with timeout, a snapshot on failure, and SIGTERM of the
      process group on `Drop`:

      ```rust
      let sitl = Px4Sitl::boot_in(&build_dir)?;
      sitl.shell("<nros module> start")?;
      sitl.shell("listener <topic>")?;              // the STOCK consumer
      let line = sitl.wait_for_log("<field marker>", RECV_TIMEOUT)
          .map_err(|e| panic!("{e:?}\n{}", sitl.log_snapshot()))?;
      ```

      `wait_for_log` is a SUBSTRING match, not a regex — the deleted test's own
      comment corrected itself about this twice, so it is worth stating once.

      This recipe is recorded here because the test it came from was **deleted**
      (issue 0356): `px4_e2e.rs` drove `nros_listener` + `nros_talker` — two
      NANO-ROS modules — and asserted one logged `recv:`. A loopback, satisfied
      identically by a correct and a broken struct layout, since both ends share
      the bug. The scaffolding was good; what it pointed at was not.

**Acceptance:** a message crosses between a nano-ros node and an unmodified PX4
module, with no serialization step on either side, and the test reads it from the
PX4 end.

**Explicitly NOT acceptance:** nano-ros subscribing its own publication. That
passes identically with a correct and a broken struct layout — it measures the
loopback, not the interop.

### W3 — the bridge: uORB → the build-time-selected RMW

- [ ] **W3.1** A PX4 module holding two sessions: uORB inward
      (`nros_rmw_uorb_register()`), and outward on the RMW chosen at build time —
      cargo `rmw-*` features / `-DNROS_RMW=<backend>`, the same knob every other
      example uses. `Executor::open_with_rmw(<name>, …)` already takes the backend
      by name; the feature picks the `register()` call and the name string.
- [ ] **W3.2** ONE path, no `<rmw>/` level and no backend pair in the directory
      name. This is phase-316's rule applied to the thing that used to break it:
      the outward backend is a build-time CHOICE, not a directory axis.
- [ ] **W3.3** Build it against **at least two** backends (zenoh + one of
      xrce/cyclonedds). One backend does not demonstrate selection; it
      demonstrates a hardcoded bridge with extra ceremony.
- [ ] **W3.4** A test with a **real ROS 2 node** subscribing the bridged topic.
      `packages/testing/nros-tests/src/ros2.rs` + `ros_env.rs` already spawn real
      ROS 2 peers for the interop cells — reuse that, do not invent a second way.

**Acceptance:** a stock PX4 module's uORB topic reaches a real ROS 2 subscriber
through the bridge, and the same source builds against a second backend.

**Not claimed:** zero-copy. The serialization uORB avoids returns at the RMW
boundary, necessarily. W2 demonstrates the zero-copy property; W3 demonstrates
reach. Conflating them would overclaim.

### W4 — the existing bridges encode their backend pair in the directory name

Not required by W1–W3, and deliberately last.

`examples/bridges/tt-zenoh-to-cyclonedds` and `tt-zenoh-to-xrce` differ only in an
outward backend the build could have chosen, with both backends named as hard
crate deps. That is the per-RMW axis phase-316 removed from paths, surviving in a
name.

- [ ] **W4.1** Decide whether they collapse to one `tt-zenoh-to-rmw` with the
      egress selected at build time, as W3's bridge is. Record the answer here
      before touching them.

**Why it matters:** if only the uORB bridge is built the right way, it reads as an
inconsistency rather than a rule, and the next bridge copies whichever neighbour
it happens to open first.

## Risks

- **W1 is the real unknown.** W2 and W3 are ordinary example code once a PX4
  module can link nano-ros; W1 is the first time anyone has tried. If it turns out
  hard, the honest move is to say so and stop — not to route around it with a
  demo that skips `nros-cpp` (which is exactly what the register-check does, and
  why this gap survived three phases).
- **Cold SITL builds are ~10 min.** Iterating on W1 means paying that repeatedly.
  Budget for it; do not shorten the loop by testing something smaller that does
  not link `nros-cpp`, because the linking IS the question.
- ~~**`just px4 test-sitl` is currently red**~~ — **cleared 2026-07-31.** Issue
  0356 resolved: `px4_e2e` removed, `test-sitl` runs Track B only and can pass.
  Track A is build-only via `build-sitl-cpp`, and `just/px4.just` +
  `examples/px4/README.md` both say so, so this phase's receipts will not be read
  against a pre-existing red.
- **Concurrent sessions.** Other agents are active; land each W in small pushed
  steps.

## Receipts to collect

| Step | Receipt |
| --- | --- |
| W0 | `just check-cpp-fmt` green without touching a PX4 module; `nros_register_check help` prints a `PRINT_MODULE_DESCRIPTION` |
| W1 | PX4 SITL module links `libnros_cpp.a`; `nm` shows resolved nano-ros symbols; module starts from pxh |
| W2 | a stock PX4 consumer (`listener <topic>`) prints a message published by the nano-ros node, asserted by a test |
| W3 | a real ROS 2 subscriber receives a stock PX4 module's uORB topic through the bridge; same source builds against a second backend |
| W4 | decision recorded here before any edit to `examples/bridges/tt-zenoh-to-*` |

## Provenance

Decisions carried from phase-316 W4, recorded there on 2026-07-31 and unchanged:

- **W4.1** — the uORB example demonstrates interop with existing PX4 features; it
  skips serialization so upstream PX4 nodes understand the message format. uORB is
  the special one.
- **W4.3** — the bridge's outward side is the build-time RMW knob, not a fixed
  backend, and the far end is a real ROS 2 node.
