# shellcheck shell=bash
# The kconfig frontend a NuttX configure needs, for every script that
# configures a NuttX tree (`build-nuttx.sh`, `make-road-build.sh`). Sourced,
# never run.
#
# NuttX's `make olddefconfig` needs a kconfig frontend. Prefer a native
# `kconfig-conf` or an already-present `kconfiglib` (`olddefconfig` on PATH);
# otherwise SELF-PROVISION kconfiglib into a repo-local venv. Issue 0431 — a
# host that ran only `nros setup <board>` has the toolchain, qemu and sources
# but NOT kconfig, so every NuttX test cell silently skipped. `pip install
# kconfiglib` (the old remedy) is refused on PEP-668 distros (Arch, Debian
# 12+); a venv's own pip is not, so the venv makes the provisioning work
# everywhere without sudo.
#
# nros_nuttx_kconfig_frontend <project-root>
#   Puts a frontend on PATH (exporting PATH), or prints why not and returns 1.
nros_nuttx_kconfig_frontend() {
    local root="$1"
    if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
        local venv="$root/build/nuttx-kconfig-venv"
        if [ ! -x "$venv/bin/olddefconfig" ]; then
            echo "kconfig frontend not found — provisioning kconfiglib into $venv …"
            if ! command -v python3 &>/dev/null; then
                echo "ERROR: no kconfig-conf/olddefconfig and no python3 to provision kconfiglib." >&2
                echo "  Install a distro package (e.g. kconfig-frontends-nox) or python3." >&2
                return 1
            fi
            if ! python3 -m venv "$venv" 2>/dev/null; then
                echo "ERROR: python3 -m venv failed — install the venv module (e.g. python3-venv)" >&2
                echo "  or a distro kconfig-frontends-nox package." >&2
                rm -rf "$venv"
                return 1
            fi
            if ! "$venv/bin/pip" install --quiet kconfiglib; then
                echo "ERROR: could not install kconfiglib into the venv (offline?)." >&2
                echo "  Install a distro kconfig-frontends-nox package instead." >&2
                rm -rf "$venv"
                return 1
            fi
        fi
        export PATH="$venv/bin:$PATH"
    fi
    if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
        echo "ERROR: kconfig tools still unavailable after provisioning (kconfig-conf/olddefconfig)." >&2
        return 1
    fi
}
