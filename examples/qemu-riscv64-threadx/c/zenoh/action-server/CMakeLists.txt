cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_c_action_server LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/threadx-riscv64-support.cmake")

nros_generate_interfaces(example_interfaces "action/Fibonacci.action" LANGUAGE C SKIP_INSTALL)

add_executable(riscv64_threadx_c_action_server src/main.c ${THREADX_STARTUP_SOURCE})
target_include_directories(riscv64_threadx_c_action_server PRIVATE ${THREADX_STARTUP_INCLUDES})
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
target_compile_definitions(riscv64_threadx_c_action_server PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
    "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
target_link_options(riscv64_threadx_c_action_server PRIVATE -T${THREADX_LINKER_SCRIPT} --nmagic --gc-sections -u app_main)
target_link_libraries(riscv64_threadx_c_action_server PRIVATE
    example_interfaces__nano_ros_c NanoRos::NanoRos threadx_platform)
