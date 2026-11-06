# shellcheck shell=bash
#
# The runner container's /tmp — ONE spelling for every container that runs jobs
# or provisioning (`runner-container.sh`, `runner-bootstrap.sh`).
#
# /tmp is a tmpfs on purpose: exec-capable scratch that dies with the container,
# so an `--ephemeral` job leaves nothing behind for the next one. The SIZE is the
# part that was wrong. It was 8g, and on 2026-11-06 a tier-2 run on the
# self-hosted runner stopped in `check::default` with
#     fatal error: error writing to /tmp/cc….s: No space left on device
# from the per-crate clippy fan-out — 9 crates' build scripts compiling C in
# parallel, every `cc` writing its assembler temp file to /tmp. The lane produced
# NO VERDICT (phase 466), which is the cost of a scratch disk sized below the job.
#
# A tmpfs `size=` is a CAP, not a reservation: pages are only taken as files are
# written, and they are swappable. So the default is raised to what a job has
# room to use on the hosts we run (32 cores, 60 GiB), and the operator — whose
# job the host's resources are — overrides it per machine:
#
#     NROS_RUNNER_TMP_SIZE=32g just runner-loop-container <labels>
#
# Two copies of this flag drifted into the two scripts before; that is why it is
# a function now.

nros_runner_tmpfs_arg() {
    local size="${NROS_RUNNER_TMP_SIZE:-24g}"
    case "$size" in
        [0-9]*[kmgKMG]|[0-9]*%) ;;
        *)
            echo "runner: NROS_RUNNER_TMP_SIZE='$size' is not a tmpfs size (e.g. 24g, 50%)" >&2
            exit 2 ;;
    esac
    printf '/tmp:rw,exec,nosuid,size=%s\n' "$size"
}
