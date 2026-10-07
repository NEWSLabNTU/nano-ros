# nano-ros — Zephyr `module_ext_root` hook (phase-481 W1, RFC-0098 D11).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# `zephyr/module.yml` declares `settings: module_ext_root: zephyr`, so Zephyr's
# `zephyr_module.cmake` includes THIS file for every application that loads the
# nano-ros module: after module discovery, BEFORE `configuration_files.cmake`
# and `kconfig.cmake`, in the application's own directory scope.
#
# That is the one point every road shares where the image's Kconfig fragment
# can still join `EXTRA_CONF_FILE`, and it is located by Zephyr's own module
# resolution, so it is always the tree the module is — the fixture road
# (`-DZEPHYR_EXTRA_MODULES`), a plain `west build` with nano-ros as a west
# project, a BYO workspace. A helper the leaf includes before
# `find_package(Zephyr)` cannot find nano-ros on the second of those (phase-481
# W0, measured).
#
# Zephyr requires a `modules/modules.cmake` in every module ext root, and that
# is all this directory is: it sets no `ZEPHYR_<MODULE>_CMAKE_DIR` (nano-ros is a
# real module with its own `zephyr/module.yml`, not glue for an external one).
# Kept to one include, so the logic lives beside the module's other cmake.

include("${CMAKE_CURRENT_LIST_DIR}/../cmake/nros_image_kconfig.cmake")
nros_image_kconfig_hook()
