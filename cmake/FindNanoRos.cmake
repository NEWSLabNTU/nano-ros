#[=======================================================================[.rst:
FindNanoRos
-----------

Find the nano-ros C library (nano-ros-c).

This is a top-level convenience module. It wraps the internal
``FindNanoRosC`` module located in ``crates/nano-ros-c/cmake/`` and
exposes a cleaner imported target name for external users.

Imported Targets
^^^^^^^^^^^^^^^^

``NanoRos::NanoRos``
  The nano-ros C library (static), with include directories and
  platform-specific link libraries already configured.

Result Variables
^^^^^^^^^^^^^^^^

``NanoRos_FOUND``
  True if nano-ros was found.

Hints
^^^^^

``NANO_ROS_ROOT``
  Path to nano-ros repository root. If not set, this module infers
  it from its own location (``<root>/cmake/FindNanoRos.cmake``).

``NANO_ROS_C_BUILD_TYPE``
  Build type: ``release`` (default) or ``debug``.

Example
^^^^^^^

.. code-block:: cmake

  list(APPEND CMAKE_MODULE_PATH "/path/to/nano-ros/cmake")
  find_package(NanoRos REQUIRED)
  target_link_libraries(my_app PRIVATE NanoRos::NanoRos)

#]=======================================================================]

# Infer NANO_ROS_ROOT from this file's location if not set
if(NOT DEFINED NANO_ROS_ROOT)
  get_filename_component(NANO_ROS_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
endif()

# Add internal cmake module path so FindNanoRosC is available
list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/crates/nano-ros-c/cmake")

# Delegate to FindNanoRosC
find_package(NanoRosC QUIET)

if(NanoRosC_FOUND)
  set(NanoRos_FOUND TRUE)

  # Create NanoRos::NanoRos as an interface that links to nano_ros_c::nano_ros_c
  if(NOT TARGET NanoRos::NanoRos)
    add_library(NanoRos::NanoRos INTERFACE IMPORTED)
    set_target_properties(NanoRos::NanoRos PROPERTIES
      INTERFACE_LINK_LIBRARIES "nano_ros_c::nano_ros_c"
    )
  endif()
else()
  set(NanoRos_FOUND FALSE)
  if(NanoRos_FIND_REQUIRED)
    message(FATAL_ERROR
      "nano-ros-c library not found.\n"
      "Make sure NANO_ROS_ROOT is set and the library is built:\n"
      "  cd ${NANO_ROS_ROOT} && cargo build -p nano-ros-c --release"
    )
  endif()
endif()
