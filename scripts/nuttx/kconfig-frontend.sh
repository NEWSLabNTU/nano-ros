# shellcheck shell=bash
# How every script here configures a NuttX tree (`build-nuttx.sh`,
# `make-road-build.sh`). Sourced, never run.
#
# A board's kernel config is a FRAGMENT over an upstream NuttX config: it
# `#include`s that config and lists only what this board does differently.
# NuttX's own `tools/configure.sh` expands it (`tools/process_config.sh`),
# then resolves it with `make olddefconfig`. configure.sh's host step
# (`tools/sethost.sh`) runs `kconfig-tweak`, and olddefconfig runs
# `kconfig-conf`. Both come from the kconfig-frontends tools, and nothing
# else ships `kconfig-tweak`, so a NuttX build REQUIRES them: on Debian and
# Ubuntu that is the `kconfig-frontends-nox` package.
#
# nros_nuttx_kconfig_frontend
#   Returns 0 when both tools are on PATH; otherwise says which are missing
#   and how to install them, and returns 1.
nros_nuttx_kconfig_frontend() {
    local missing=() tool
    for tool in kconfig-conf kconfig-tweak; do
        command -v "$tool" &>/dev/null || missing+=("$tool")
    done
    [ "${#missing[@]}" -eq 0 ] && return 0
    {
        echo "ERROR: configuring a NuttX kernel needs the kconfig-frontends tools,"
        echo "  and this host has no ${missing[*]}."
        echo "  Install them, then re-run the build:"
        echo "    Debian/Ubuntu:  sudo apt install kconfig-frontends-nox"
        echo "    elsewhere:      build kconfig-frontends from source, as NuttX's"
        echo "                    own setup guide describes"
        echo "  (\`nros setup <nuttx board> --sudo\` installs them too.)"
    } >&2
    return 1
}

# nros_nuttx_configure <nuttx-dir> <apps-dir> <defconfig> <board-makedefs> <stage-dir>
#   Configure <nuttx-dir> from the board fragment <defconfig> with NuttX's
#   own `tools/configure.sh`. The caller distcleans first: configure.sh
#   refuses a tree that is already configured, and the caller may have apps
#   to stage between the distclean and the configure.
#   configure.sh takes an out-of-tree board config as a DIRECTORY that holds
#   `defconfig` with `Make.defs` beside it. So the fragment is staged into
#   <stage-dir> next to a link to the board's <board-makedefs> (an absolute
#   path into the kernel's own boards/ tree).
#   <apps-dir> reaches configure.sh relative to <nuttx-dir>, which is the only
#   form it accepts.
nros_nuttx_configure() {
    local nuttx_dir="$1" apps_dir="$2" defconfig="$3" makedefs="$4" stage="$5"
    nros_nuttx_kconfig_frontend || return 1
    if [ ! -f "$makedefs" ]; then
        echo "ERROR: no board Make.defs at $makedefs" >&2
        return 1
    fi
    mkdir -p "$stage"
    cp "$defconfig" "$stage/defconfig"
    ln -sfn "$makedefs" "$stage/Make.defs"
    local apps_rel
    apps_rel="$(realpath --relative-to="$nuttx_dir" "$apps_dir")"
    (
        cd "$nuttx_dir" || exit 1
        rm -f .config Make.defs defconfig
        tools/configure.sh -a "$apps_rel" "$stage"
    )
}
