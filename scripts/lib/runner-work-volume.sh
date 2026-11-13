# shellcheck shell=bash
#
# The runner's job-tree volume, keyed by ROLE.
#
#   nros_runner_work_volume <labels>
#
# Prints `nros-runner-work-queue` when <labels> (comma or space separated)
# carries `nros-queue`, else `nros-runner-work`.
#
# A host may run two runner loops at once: the merge queue's runner and a
# scheduled-lane runner (`docs/development/multi-agent-ci-workflow.md`, "The
# merge queue has its own runners"). Both mount `/home/runner/_work`, and two
# jobs sharing one `_work` tree delete each other's checkouts. Keying by role
# rather than by runner name means one host never grows more than two job trees,
# and a renamed runner keeps its tree. Everything else (the SDK store, cargo,
# sccache, the src checkout) is shared on purpose: it is read-mostly and keyed
# by content, and a second copy of a 9 GB SDK buys nothing.
#
# ONE spelling, for the job container, the sweep container and the bootstrap.
# A sweep that mounted a different volume would measure a different disk.
nros_runner_work_volume() {
    case ",$(printf '%s' "${1:-}" | tr ' ' ','),"
    in
        *,nros-queue,*) printf '%s\n' "nros-runner-work-queue" ;;
        *)              printf '%s\n' "nros-runner-work" ;;
    esac
}
