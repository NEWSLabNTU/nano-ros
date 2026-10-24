# Refuse a Zephyr image that mixes TWO nano-ros checkouts — issue 1379.
#
# ## The failure this exists for
#
# A Zephyr image takes its `nros` module from west's project list. A workspace
# provisioned before phase-449 W1 binds that list to whichever checkout ran
# `just zephyr setup` (a `nano-ros` symlink at the workspace root), and since
# phase-440 W4 the default workspace is `$NROS_HOME/workspaces/zephyr/<v>` —
# one directory shared by every checkout on the host. So a build launched from
# checkout B compiles checkout A's `zephyr/` module, platform sources and
# nros-c/nros-cpp headers, beside the entry code B generated. Measured, in the
# build dir issue 1379 was filed from:
#
#   APPLICATION_SOURCE_DIR = <worktree>/examples/zephyr/rust/listener
#   NROS_REPO_DIR          = <another clone>/third-party/nano-ros
#
# Nothing warned. A half-and-half image is not a result about either tree, and
# it is worse than a loud failure because a test run against it sends the next
# person after a defect that belongs to a checkout nobody was looking at.
#
# ## Why the guard lives HERE and not only in the builders
#
# The builders now all pass `-DZEPHYR_EXTRA_MODULES=<this checkout>`
# (`scripts/lib/zephyr-module.sh`, gated by `check-zephyr-module-binding`), and
# a static gate keeps the next one from being written without it. But a `west
# build` typed by hand reaches none of that, and an out-of-tree consumer reaches
# the module through its own CMakeLists. Configure time is the one place EVERY
# route passes through, so the refusal is placed where the mixing becomes real
# rather than at each of the places it can be arranged.
#
# ## The rule is three-valued, exactly as `scripts/lib/checkout-paths.sh` states
# ## it for inherited SDK paths (issue 1280)
#
#   the application source dir is …        | behaviour
#   ---------------------------------------|----------------------------------
#   outside any nano-ros checkout          | ACCEPT — a copied-out example
#                                          | (`check-copy-out.sh`) or a
#                                          | downstream project; there is no
#                                          | second tree for it to disagree with
#   inside THIS module's checkout          | ACCEPT — the ordinary case
#   inside a DIFFERENT nano-ros checkout   | REFUSE, naming both
#
# "Which checkout does this path belong to" is answered by walking up for the
# tree's one checkout marker (`packages/core/nros-core/Cargo.toml`), never by
# `.git`: a linked worktree's `.git` is a FILE (issue 1336), and worktrees are
# how parallel sessions are run here — which is exactly the population this
# defect was found in.

# The nano-ros checkout a path belongs to, or "" when it belongs to none.
# Purely lexical, so it attributes a directory that has not been provisioned.
function(_nros_checkout_root_of path out_var)
    set(_dir "${path}")
    while(_dir AND NOT _dir STREQUAL "/")
        if(EXISTS "${_dir}/packages/core/nros-core/Cargo.toml")
            set(${out_var} "${_dir}" PARENT_SCOPE)
            return()
        endif()
        get_filename_component(_parent "${_dir}" DIRECTORY)
        if(_parent STREQUAL _dir)
            break()
        endif()
        set(_dir "${_parent}")
    endwhile()
    set(${out_var} "" PARENT_SCOPE)
endfunction()

# `module_root` is the nano-ros checkout supplying this `nros` module
# (`NROS_REPO_DIR`); `app_dir` is the application being built
# (`APPLICATION_SOURCE_DIR`).
function(nros_assert_single_checkout module_root app_dir)
    if(NROS_ALLOW_FOREIGN_MODULE)
        message(WARNING
            "nano-ros: NROS_ALLOW_FOREIGN_MODULE is set — not checking whether "
            "this image mixes two nano-ros checkouts (issue 1379).")
        return()
    endif()

    # REALPATH on both sides: `NROS_REPO_DIR` is `<module>/zephyr/..`, and a
    # workspace symlinked onto another disk reaches the same second checkout as
    # an absolute path does.
    get_filename_component(_module_real "${module_root}" REALPATH)
    get_filename_component(_app_real "${app_dir}" REALPATH)

    _nros_checkout_root_of("${_app_real}" _app_checkout)
    if(NOT _app_checkout)
        return() # row 1 — nothing to disagree with
    endif()

    _nros_checkout_root_of("${_module_real}" _module_checkout)
    if(NOT _module_checkout)
        # The module is not inside a checkout at all (an installed/vendored
        # copy). Not this check's case, and refusing it would break a consuming
        # project that vendors the module without the workspace.
        return()
    endif()

    if(_app_checkout STREQUAL _module_checkout)
        return() # row 2 — the ordinary case
    endif()

    message(FATAL_ERROR
        "nano-ros: this image would mix TWO nano-ros checkouts (issue 1379).\n"
        "  application:   ${_app_real}\n"
        "    its checkout: ${_app_checkout}\n"
        "  nros module:   ${_module_real}\n"
        "    its checkout: ${_module_checkout}\n"
        "\n"
        "The `nros` Zephyr module — zephyr/, the platform C sources, the nros-c "
        "and nros-cpp headers — would come from the second tree, while the entry "
        "code comes from the first. Whatever commit that second tree is at "
        "decides the result, so neither a pass nor a failure would be a fact "
        "about the tree you are building.\n"
        "\n"
        "This usually means the west workspace's manifest project is still a "
        "symlink into the checkout that provisioned it (issue 1258). Two fixes, "
        "either is enough:\n"
        "  * pass -DZEPHYR_EXTRA_MODULES=<your checkout> to `west build`\n"
        "    (`scripts/lib/zephyr-module.sh cmake-arg` prints exactly that), or\n"
        "  * unbind the workspace once, in place:\n"
        "    bash scripts/zephyr/unbind-manifest-project.sh <workspace>\n"
        "\n"
        "-DNROS_ALLOW_FOREIGN_MODULE=ON downgrades this to a warning. Say why: a "
        "deliberately mixed image is a measurement of neither tree.")
endfunction()

# Refuse a Zephyr SDK that lives inside ANOTHER nano-ros checkout — phase-478 I2.
#
# The same three rows as above, asked of the toolchain instead of the
# application. Before issue 1254 moved the SDK into the store, `just zephyr
# setup` installed it under `<checkout>/scripts/zephyr/sdk/`, and a shared west
# workspace's `env.sh` exported that path to every checkout on the host: on
# 2026-10-05 both lines of this host's shared workspace compiled every image
# with the safety island's vendored submodule's SDK copy. A workspace
# provisioned then, or an `env.sh` copied from one, still does that, and the
# image is compiled by a toolchain whose version is whatever that other tree
# pinned.
#
#   the SDK dir is …                       | behaviour
#   ---------------------------------------|----------------------------------
#   outside any nano-ros checkout          | ACCEPT — the store
#                                          | (`~/.nros/sdk/zephyr-sdk/<v>`) or
#                                          | a user's own install
#   inside THIS module's checkout          | ACCEPT — the legacy checkout arm
#                                          | of `scripts/lib/zephyr-sdk.sh`
#   inside a DIFFERENT nano-ros checkout   | REFUSE, naming both
#
# Ownership, not `nros-sdk.lock`, decides it: the lock records VERSIONS, and the
# defect is a PATH — a store SDK at a version the lock does not name is
# `nros_cli_core::orchestration::drift`'s `tool` kind, a different question.
function(nros_assert_sdk_not_foreign module_root sdk_dir)
    if(NOT sdk_dir)
        return() # no Zephyr SDK toolchain (a host or GNU Arm build)
    endif()
    if(NROS_ALLOW_FOREIGN_SDK)
        message(WARNING
            "nano-ros: NROS_ALLOW_FOREIGN_SDK is set — not checking whether the "
            "Zephyr SDK belongs to another nano-ros checkout (phase-478 I2).")
        return()
    endif()
    get_filename_component(_sdk_real "${sdk_dir}" REALPATH)
    _nros_checkout_root_of("${_sdk_real}" _sdk_checkout)
    if(NOT _sdk_checkout)
        return() # row 1
    endif()
    get_filename_component(_module_real "${module_root}" REALPATH)
    _nros_checkout_root_of("${_module_real}" _module_checkout)
    if(NOT _module_checkout OR _sdk_checkout STREQUAL _module_checkout)
        return() # a vendored module, or row 2
    endif()
    message(FATAL_ERROR
        "nano-ros: the Zephyr SDK belongs to ANOTHER nano-ros checkout (phase-478 I2).\n"
        "  SDK in use:    ${_sdk_real}\n"
        "    its checkout: ${_sdk_checkout}\n"
        "  nros module:   ${_module_real}\n"
        "    its checkout: ${_module_checkout}\n"
        "\n"
        "This image would be compiled by whatever toolchain that other tree "
        "installed. The SDK belongs in the shared store, where every checkout "
        "finds the version its own index pins (issue 1254). Re-register it from "
        "THIS checkout, then reconfigure:\n"
        "    just zephyr setup\n"
        "or point the build at the store copy:\n"
        "    export ZEPHYR_SDK_INSTALL_DIR=\"$(bash scripts/lib/zephyr-sdk.sh resolve)\"\n"
        "\n"
        "-DNROS_ALLOW_FOREIGN_SDK=ON downgrades this to a warning. Say why.")
endfunction()
