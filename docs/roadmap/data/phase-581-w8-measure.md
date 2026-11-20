# phase-581 W8 - measurements: the open RMW set

Date: 2026-11-20. Host: the `ros2` distrobox (Ubuntu 22.04, CMake 3.22.1,
Python 3.10.12), Zephyr 3.7.0 from the box's `~/.nros-box/workspaces/zephyr/3.7`,
`nros` built by `just setup-cli` in the W8 worktree. Every number below comes
from a command that was run; what was not run says NOT MEASURED and why.

## 1. A package that is its own Zephyr module (acceptance)

Fixture: `packages/testing/nros-tests/fixtures/zephyr-module-rmw`, its tracked
files copied to a scratch workspace outside the checkout (a fresh clone's
view). `src/stub_rmw` announces `rmw stub`; its `nros-rmw.toml` states
`[rmw.zephyr] module = "zephyr"`; `src/stub_rmw/zephyr/` holds `module.yml`, a
`Kconfig` (`STUB_RMW_TRANSPORT`, which depends on `NROS_RMW_STUB`, and the
member's kernel heap default) and a `CMakeLists.txt` that prints a line and
compiles `stub_transport.c`. The image is `[image.zephyr] board = "zephyr"`
(`native_sim/native/64`), `rmw = "stub"`.

Command (from the Zephyr 3.7 workspace, `ZEPHYR_TOOLCHAIN_VARIANT=host`):

```
nros build demo_bringup:zephyr --workspace <copy> -- -d <build dir>
```

The caller passes no module: `nros build`'s west command line names
`-DZEPHYR_EXTRA_MODULES=<nano-ros>` only. The configure log:

```
-- nano-ros: image `zephyr`'s RMW is its own Zephyr module -> <copy>/src/stub_rmw added to this build's modules
...
-- nano-ros: image demo_bringup:zephyr (rmw=stub) <- <copy>
-- stub_rmw: the stub RMW package's own Zephyr module CMake runs (CONFIG_NROS_RMW_STUB=y, CONFIG_STUB_RMW_TRANSPORT=y)
```

What the build directory holds afterwards:

| file | content |
| --- | --- |
| `zephyr_modules.txt` | 10 modules, one of them `"stub_rmw":"<copy>/src/stub_rmw":"<copy>/src/stub_rmw/zephyr"` |
| `Kconfig/Kconfig.modules` | `osource "<copy>/src/stub_rmw/zephyr/Kconfig"` (line 47, after nano-ros's at line 41) |
| `nros/Kconfig.packages` | `choice NROS_RMW_BACKEND` / `config NROS_RMW_STUB` / `endchoice` |
| `zephyr/.config` | `CONFIG_NROS_RMW_STUB=y`, `# CONFIG_NROS_RMW_ZENOH is not set`, `# CONFIG_NROS_RMW_XRCE is not set`, `CONFIG_STUB_RMW_TRANSPORT=y`, `CONFIG_ZEPHYR_STUB_RMW_MODULE=y`, `CONFIG_HEAP_MEM_POOL_SIZE=8192` |
| `modules/stub_rmw/CMakeFiles/stub_rmw_transport.dir/stub_transport.c.obj` | present |
| `zephyr/zephyr.exe` | linked (the build ran to the end, exit 0) |

Result: GREEN. The configure added the package's module, Kconfig took its
member into the module's choice, the image's `rmw` selected it, and the
module's CMake ran, with no edit outside the fixture.

Two things the run found, both kept:

- **The kernel heap.** The first build failed in Zephyr's NSOS driver
  (`nsos_sockets.c:38: BUILD_ASSERT(CONFIG_HEAP_MEM_POOL_SIZE > 0)`): the
  nano-ros module sizes `HEAP_MEM_POOL_SIZE` per member of its choice
  (`configdefault` under `if NROS_RMW_ZENOH` and `if NROS_RMW_CYCLONEDDS`), so
  a member from another module got Zephyr's 0. The fixture's own module states
  `configdefault HEAP_MEM_POOL_SIZE default 8192 if NROS_RMW_STUB`; the rebuild
  passed. A real module-providing RMW has to state the Zephyr facts its
  transport needs, as the in-tree members do in `zephyr/Kconfig`.
- **The module list is re-read, not re-declared.** The module hook runs
  inside Zephyr's `zephyr_module.cmake`, after `zephyr_module.py` has listed
  the modules and before the list is read into `ZEPHYR_<NAME>_CMAKE_DIR` and
  `Kconfig.modules` is sourced. Setting `ZEPHYR_EXTRA_MODULES` there would be
  read by nothing, so the hook runs `zephyr_module.py` again with the root
  added to `--extra-modules`, and re-reads its outputs (the module list, and
  any new board, DTS, SoC or snippet root). Cost: one more `zephyr_module.py`
  run per configure, only for an image whose RMW brings a module.

Zephyr 4.4: NOT MEASURED. The box's CMake finds Python 3.10.12 and Zephyr
4.4's `python.cmake` requires 3.12, so a 4.4 configure stops before any module
code (`Could NOT find Python3: Found unsuitable version "3.10.12"`). The 4.4
`zephyr_module.cmake` was read: the `zephyr_module.py` call, the outputs and
the order (list, ext-root hooks, then the module loop) are the same as 3.7's,
so the same hook applies; W0's M5 measured the choice-member shape on 4.4.

## 2. The refusal of another member in a conf

On the build directory of section 1 (image `rmw = "stub"`), each re-configured
with `cmake <build dir>`:

| added | result |
| --- | --- |
| `-DEXTRA_CONF_FILE=<file holding CONFIG_NROS_RMW_ZENOH=y>` | configure error at `nros_image_kconfig.cmake`: `image `zephyr` selects NROS_RMW_STUB (its `rmw` in .../system.toml), and a conf assigns another member of the RMW choice: <file>: `CONFIG_NROS_RMW_ZENOH=y`` |
| `-DCONFIG_NROS_RMW_ZENOH=y` | the same error, naming `-DCONFIG_NROS_RMW_ZENOH=y on the command line` |

Without the refusal the rendered fragment (merged last) would have kept
`NROS_RMW_STUB` and dropped the user's line with no word (W0's M5).

## 3. `acme`, built over the zenoh member

Fixture `out-of-tree-rmw`, image `zephyr`, the same command on a fresh build
directory: exit 0 in 159 s, `zephyr/zephyr.exe` linked (6.4 MB),
`CONFIG_NROS_RMW_ZENOH=y`, `CONFIG_NROS_ACME_POOL_SLOTS=4`. Its
`Kconfig.packages` adds no choice member (acme is not a module of its own), and
no module was added. Run, not just built: NOT MEASURED here (the e2e test
`out_of_tree_rmw_zephyr_e2e.rs` runs the lane's image).

## 4. Tests

| test | result |
| --- | --- |
| `cargo-nano-ros` lib (235), incl. `the_zephyr_members_are_derived_from_the_scan`, `a_module_backend_names_a_member_of_the_scan`, `a_backend_names_its_own_zephyr_module`, `a_multiline_string_is_skipped` | pass |
| `nros-cli-core` `leaf_kconfig`/`leaf_system`/`image_facts` (39), incl. `the_derived_members_are_the_rendered_choice` (RFC-0071 D18's test) and `a_zephyr_module_backend_is_its_own_member` | pass |
| `nros-cli-core --test out_of_tree_rmw_build` (4), incl. `an_rmw_package_that_is_its_own_zephyr_module_joins_the_choice` | pass, 142 s |
| `scripts/gen-rmw-choice.py --check` | OK, zephyr 3 and nuttx 3 members, 5 negative controls caught |
