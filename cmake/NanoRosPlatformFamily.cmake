# NanoRosPlatformFamily.cmake — the ONE map from a NANO_ROS_PLATFORM spelling to
# its RTOS family (issue 1795).
#
#   nros_platform_family(<out_var> <platform>)
#
#   threadx_linux / threadx_riscv64 -> threadx
#   freertos_armcm3                  -> freertos
#   nuttx_armv7a                     -> nuttx
#   anything else                    -> itself
#
# The just recipes and toolchain files pass the VARIANT spelling
# (`-DNANO_ROS_PLATFORM=threadx_riscv64`), while a build ROAD is chosen per
# family. `nano_ros_entry()` normalised inline; the three RTOS typed-entry
# carriers in NanoRosNodeRegister.cmake compared the raw value to the bare
# family token, so on every lane-built rv-virt-threadx leaf the ThreadX carrier
# arm never fired — no image, no panic policy, and a `nros-c` staticlib built
# with no `#[panic_handler]` (issue 1795, the first time a lane built one).
# Gate: `check-road-dispatch-platform-family` — the road dispatchers compare
# families, never a raw family token, and this is the only normaliser.
include_guard(GLOBAL)

function(nros_platform_family out_var platform)
    set(_f "${platform}")
    if(_f MATCHES "^(threadx|freertos|nuttx)_")
        string(REGEX REPLACE "_.*$" "" _f "${_f}")
    endif()
    set(${out_var} "${_f}" PARENT_SCOPE)
endfunction()
