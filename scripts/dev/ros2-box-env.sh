# Source this INSIDE a distrobox/container/VM before working on nano-ros there.
#
#   . scripts/dev/ros2-box-env.sh
#
# WHAT THIS IS FOR, AND WHAT IT IS NO LONGER FOR (issue 1248)
#
# The build system knows nothing about distrobox, containers or VMs, and must
# not: the only question it may ask about an environment is whether ROS 2 ament
# packages are discoverable, so message packages can be found. WHERE that Linux
# runs is the operator's business.
#
# This file survives that rule for ONE reason, and it is not a box concept. A
# distrobox SHARES `$HOME` with the host by design, so `~/.nros` and `~/.cargo`
# are the SAME directory in both — two toolchains, two libcs, one store. That is
# the general rule "a store belongs to one toolchain" meeting an environment that
# breaks the usual assumption that a different machine has a different `$HOME`.
# So all this does is give the inner toolchain its own store.
#
# What it USED to do, and why that is gone: it redirected `CARGO_TARGET_DIR` and
# carried an `NROS_ALLOW_SHARED_BOX_TREE` escape so a box could work in the
# host's tree. Issue 0759 already refused that by default (shared artifacts,
# different compiler and libc, nothing checking they agree), and this file's own
# comment said redirecting in a box-owned tree is "actively harmful" — the
# fixture contract is LEAF-RELATIVE, so a redirect moves fixtures out from under
# the tests that stat them. Nothing needs the mode, so nothing keeps the knobs.
#
# THE RULE, one line: build where you run. Clone nano-ros INSIDE the box and
# work there. Do not build on the host and run in the box, and do not mirror one
# tree into the other -- `ros2-box-sync.sh` did the latter and is retired with
# this change (issue 1248); every bug it ever had was a bug in the copy.

_nros_box_root="$(cd -P "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd -P)"

# The store, the cargo install root and the PATH that reaches it. `${X:-...}`
# throughout, so an operator who has already separated these keeps their own
# values -- this is a default, not a policy.
export NROS_HOME="${NROS_HOME:-$HOME/.nros-box}"
export CARGO_INSTALL_ROOT="${CARGO_INSTALL_ROOT:-$HOME/.local-box}"
export PATH="$CARGO_INSTALL_ROOT/bin:$PATH"

# A host-built binary cannot run here (different glibc), and the failure is a
# loader error naming a symbol version, which reads like a code bug. Say so once,
# at the point where it is still cheap to notice.
if [ -x "$HOME/.nros/bin/nros" ] && [ "$NROS_HOME" = "$HOME/.nros-box" ]; then
    echo "ros2-box-env: using $NROS_HOME (the host's ~/.nros is a different toolchain's store)" >&2
fi

unset _nros_box_root
