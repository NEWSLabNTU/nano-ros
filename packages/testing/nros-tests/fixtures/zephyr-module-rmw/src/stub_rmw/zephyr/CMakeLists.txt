# The `stub` RMW package's own Zephyr module CMake (phase-581 W8). The line
# below is what the acceptance reads: this file ran in the image's configure.
message(STATUS "stub_rmw: the stub RMW package's own Zephyr module CMake runs "
    "(CONFIG_NROS_RMW_STUB=${CONFIG_NROS_RMW_STUB}, "
    "CONFIG_STUB_RMW_TRANSPORT=${CONFIG_STUB_RMW_TRANSPORT})")

if(CONFIG_STUB_RMW_TRANSPORT)
    zephyr_library_named(stub_rmw_transport)
    zephyr_library_sources(stub_transport.c)
endif()
