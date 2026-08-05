# PX4 EXTERNAL_MODULES_LOCATION entry point for the nano-ros uORB→RMW bridge
# (phase-325 W3, issue 0362).
#
# Pass this directory's parent (`examples/px4/cpp/bridge/`) as
# EXTERNAL_MODULES_LOCATION. PX4 takes exactly ONE such location per build, which
# is why this is its own root rather than a second module under the W2 demo.

set(config_module_list_external
    modules/nros_uorb_bridge
    PARENT_SCOPE
)
