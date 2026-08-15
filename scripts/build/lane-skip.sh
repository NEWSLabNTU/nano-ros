# Lane skip protocol — issue 0599.
#
# A platform fixture lane that cannot run because a PRECONDITION is missing (no
# Zephyr workspace, no `arm-none-eabi-gcc`, no PX4 tree) is not a failure. It was
# also not a success, and every such site used to `exit 0`, so the driver
# recorded `== zephyr == OK` for a lane that built nothing.
#
# What that cost, concretely: the four west-owned compile-check fixtures never
# got built, and the operator learned it twenty minutes later from `_lane-gate`,
# as four missing `.inputsig` files, with a remedy (`just build-test-fixtures`)
# naming the command that had just "succeeded". A skip invisible at the point of
# decision surfaces as an artifact error at a distance from its cause.
#
# So: a third verdict. `nros_lane_skip "<reason>"` prints a machine-readable
# marker and exits 78 (sysexits' EX_CONFIG — "configuration error", which is
# exactly what a missing SDK is). The driver in `justfile`'s
# `build-test-fixtures-leaves` treats 78 as SKIPPED, prints the reason, and does
# NOT fail the build.
#
# ONE spelling, because six sites across three lanes had the same `exit 0` and
# fixing one would have left five. Add new skip sites through this function.

NROS_LANE_SKIP_RC=78

# nros_lane_skip <reason…>
#
# Exits the calling lane with the SKIPPED verdict. The reason is printed twice
# on purpose: once as prose for whoever reads the lane log directly, once as the
# `NROS_LANE_SKIP:` marker the driver greps out of that log to put in the
# summary. Keep the reason short and name the remedy — it is what the operator
# sees instead of "OK".
nros_lane_skip() {
    local reason="$*"
    echo "NROS_LANE_SKIP: ${reason}"
    echo "lane skipped: ${reason}"
    exit "${NROS_LANE_SKIP_RC}"
}
