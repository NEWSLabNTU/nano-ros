# PX4 EXTERNAL_MODULES_LOCATION entry point for nano-ros uORB examples.
#
# Pass this directory's parent (`examples/px4/rust/uorb/`) as
# `EXTERNAL_MODULES_LOCATION` to PX4's `make`. PX4 looks at
# `<location>/src/CMakeLists.txt` (this file) and pulls in each
# subdirectory listed in `config_module_list_external`.
#
# Each `modules/<name>/CMakeLists.txt` is a thin wrapper that points
# `px4_rust_module()` at the corresponding crate's own `Cargo.toml`
# under `examples/px4/rust/uorb/<name>/`. The crates remain the source
# of truth — easy to copy out of nano-ros into a user's own
# `EXTERNAL_MODULES_LOCATION` if they want.

set(config_module_list_external
    modules/nros_talker
    modules/nros_listener
    PARENT_SCOPE
)
