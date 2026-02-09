#[=======================================================================[.rst:
FindNanoRosCodegen
------------------

Find (or build) the bundled nano-ros C code generation tool.

This module locates ``libnano_ros_codegen_c.a`` and its companion C wrapper
source, then uses ``try_compile`` to produce a self-contained executable at
CMake configure time.  No external ``nano-ros`` binary is required.

Result Variables
^^^^^^^^^^^^^^^^

``NanoRosCodegen_FOUND``
  True if the codegen tool was built successfully.

``_NANO_ROS_CODEGEN_TOOL``
  Path to the built codegen executable (cache variable).

Hints
^^^^^

``NANO_ROS_ROOT``
  Path to the nano-ros repository root.

#]=======================================================================]

if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL})
  set(NanoRosCodegen_FOUND TRUE)
  return()
endif()

# --- Locate NANO_ROS_ROOT ---------------------------------------------------
if(NOT DEFINED NANO_ROS_ROOT)
  get_filename_component(NANO_ROS_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
endif()

# --- Locate the static library ----------------------------------------------
set(_codegen_pkg_dir "${NANO_ROS_ROOT}/packages/codegen/packages/nano-ros-codegen-c")

# Prefer release, fall back to debug
set(_lib_path "")
foreach(_profile release debug)
  set(_candidate "${NANO_ROS_ROOT}/packages/codegen/packages/target/${_profile}/libnano_ros_codegen_c.a")
  if(EXISTS "${_candidate}")
    set(_lib_path "${_candidate}")
    break()
  endif()
endforeach()

if(NOT _lib_path)
  set(NanoRosCodegen_FOUND FALSE)
  if(NanoRosCodegen_FIND_REQUIRED)
    message(FATAL_ERROR
      "libnano_ros_codegen_c.a not found.\n"
      "Build it with:\n"
      "  cd ${NANO_ROS_ROOT} && cargo build -p nano-ros-codegen-c --release "
      "--manifest-path packages/codegen/packages/Cargo.toml"
    )
  endif()
  return()
endif()

# --- Locate header and wrapper source ----------------------------------------
set(_header_dir "${_codegen_pkg_dir}/include")
set(_wrapper_src "${_codegen_pkg_dir}/src/codegen_main.c")

if(NOT EXISTS "${_header_dir}/nano_ros_codegen.h")
  message(FATAL_ERROR "nano_ros_codegen.h not found at ${_header_dir}")
endif()
if(NOT EXISTS "${_wrapper_src}")
  message(FATAL_ERROR "codegen_main.c not found at ${_wrapper_src}")
endif()

# --- Build the wrapper executable via try_compile ----------------------------
set(_codegen_bin_dir "${CMAKE_BINARY_DIR}/_nano_ros_codegen")
file(MAKE_DIRECTORY "${_codegen_bin_dir}")

# Detect platform link libraries required by the Rust staticlib
set(_platform_libs "")
if(UNIX AND NOT APPLE)
  set(_platform_libs "-lpthread -ldl -lm")
elseif(APPLE)
  set(_platform_libs "-lpthread -ldl -lm -framework Security -framework CoreFoundation")
endif()

try_compile(_codegen_compiled
  "${_codegen_bin_dir}"
  SOURCES "${_wrapper_src}"
  CMAKE_FLAGS
    "-DINCLUDE_DIRECTORIES=${_header_dir}"
    "-DLINK_LIBRARIES=${_lib_path};${_platform_libs}"
  COPY_FILE "${_codegen_bin_dir}/nano_ros_codegen"
  OUTPUT_VARIABLE _codegen_output
)

if(NOT _codegen_compiled)
  set(NanoRosCodegen_FOUND FALSE)
  if(NanoRosCodegen_FIND_REQUIRED)
    message(FATAL_ERROR
      "Failed to compile nano_ros_codegen wrapper.\n"
      "Output:\n${_codegen_output}"
    )
  endif()
  return()
endif()

set(_NANO_ROS_CODEGEN_TOOL "${_codegen_bin_dir}/nano_ros_codegen"
  CACHE INTERNAL "Path to nano-ros C codegen tool")

set(NanoRosCodegen_FOUND TRUE)
message(STATUS "Built nano-ros codegen tool: ${_NANO_ROS_CODEGEN_TOOL}")
