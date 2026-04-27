# PX4 picks up `config_msg_list_external` and appends each .msg in this
# directory to the canonical message-codegen list. Required for any
# custom topic NOT already in PX4-Autopilot's `msg/` tree — without
# this, `px4_rs_find_orb_meta()` returns NULL for the topic name and
# the Rust crate falls back to a static OrbMetadata whose `o_id` is
# `u16::MAX`. PX4's broker reads `o_id` as an index into its compiled-
# in topic table; the sentinel value triggers an out-of-bounds access
# that crashes the daemon as soon as a subscriber registers.
#
# Listing the .msg here causes PX4's build to assign a valid `o_id`
# and register the topic in its canonical table, so the runtime
# `px4_rs_find_orb_meta("sensor_ping")` returns a real pointer that
# both publishers and subscribers resolve through.
set(config_msg_list_external
    SensorPing.msg
    PARENT_SCOPE
)
