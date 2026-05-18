# NanoRosConfig.cmake
#
# CMake helper to read config.toml and set compile definitions.
#
# Usage:
#   include("${PROJECT_SOURCE_DIR}/cmake/NanoRosConfig.cmake")
#   nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
#   # Now use: NROS_CONFIG_IP, NROS_CONFIG_MAC, NROS_CONFIG_GATEWAY,
#   #          NROS_CONFIG_NETMASK, NROS_CONFIG_PREFIX,
#   #          NROS_CONFIG_ZENOH_LOCATOR, NROS_CONFIG_DOMAIN_ID

# nano_ros_read_config(<config_file>)
#
# Reads a config.toml file and sets variables in the parent scope:
#   NROS_CONFIG_IP            - e.g. "192,0,3,10" (C array initializer)
#   NROS_CONFIG_MAC           - e.g. "0x02,0x00,0x00,0x00,0x00,0x00"
#   NROS_CONFIG_GATEWAY       - e.g. "192,0,3,1"
#   NROS_CONFIG_NETMASK       - e.g. "255,255,255,0"
#   NROS_CONFIG_PREFIX        - e.g. "24"
#   NROS_CONFIG_ZENOH_LOCATOR - e.g. "tcp/192.0.3.1:7447"
#   NROS_CONFIG_DOMAIN_ID     - e.g. "0"
#
function(nano_ros_read_config CONFIG_FILE)
    if(NOT EXISTS "${CONFIG_FILE}")
        message(FATAL_ERROR "nano_ros_read_config: ${CONFIG_FILE} not found")
    endif()

    file(READ "${CONFIG_FILE}" _content)

    # Defaults — network
    set(_ip "192,0,3,10")
    set(_mac "0x02,0x00,0x00,0x00,0x00,0x00")
    set(_gateway "192,0,3,1")
    set(_netmask "255,255,255,0")
    set(_prefix "24")
    set(_locator "tcp/127.0.0.1:7447")
    set(_domain_id "0")
    set(_interface "")

    # Defaults — scheduling (normalized 0–31 scale)
    set(_app_priority "12")
    # 256 KB — FreeRTOS QEMU zenoh session open can exceed 160 KiB with lwIP.
    # Keep the C/C++ generated config in sync with the Rust board default so
    # stack overflow checks fail cleanly instead of corrupting the task state.
    set(_app_stack_bytes "262144")
    set(_zenoh_read_priority "16")
    set(_zenoh_read_stack_bytes "5120")
    set(_zenoh_lease_priority "16")
    set(_zenoh_lease_stack_bytes "5120")
    set(_poll_priority "16")
    set(_poll_interval_ms "5")

    # Track current section
    set(_section "")

    # Parse line by line
    string(REPLACE "\n" ";" _lines "${_content}")
    foreach(_line IN LISTS _lines)
        string(STRIP "${_line}" _line)

        # Skip empty lines and comments
        if("${_line}" STREQUAL "" OR "${_line}" MATCHES "^#")
            continue()
        endif()

        # Section header
        if("${_line}" MATCHES "^\\[([a-z]+)\\]")
            set(_section "${CMAKE_MATCH_1}")
            continue()
        endif()

        # Key = value
        if("${_line}" MATCHES "^([a-z_]+)[ \t]*=[ \t]*(.*)")
            set(_key "${CMAKE_MATCH_1}")
            set(_val "${CMAKE_MATCH_2}")
            # Strip quotes
            if("${_val}" MATCHES "^\"(.*)\"$")
                set(_val "${CMAKE_MATCH_1}")
            endif()

            # [network] section
            if("${_section}" STREQUAL "network")
                if("${_key}" STREQUAL "ip")
                    _nros_ip_to_c("${_val}" _ip)
                elseif("${_key}" STREQUAL "mac")
                    _nros_mac_to_c("${_val}" _mac)
                elseif("${_key}" STREQUAL "gateway")
                    _nros_ip_to_c("${_val}" _gateway)
                elseif("${_key}" STREQUAL "netmask")
                    _nros_ip_to_c("${_val}" _netmask)
                elseif("${_key}" STREQUAL "prefix")
                    set(_prefix "${_val}")
                    _nros_prefix_to_netmask("${_val}" _netmask)
                endif()
            # [zenoh] section
            elseif("${_section}" STREQUAL "zenoh")
                if("${_key}" STREQUAL "locator")
                    set(_locator "${_val}")
                elseif("${_key}" STREQUAL "domain_id")
                    set(_domain_id "${_val}")
                endif()
            # [platform] section
            elseif("${_section}" STREQUAL "platform")
                if("${_key}" STREQUAL "interface")
                    set(_interface "${_val}")
                endif()
            # [scheduling] section
            elseif("${_section}" STREQUAL "scheduling")
                if("${_key}" STREQUAL "app_priority")
                    set(_app_priority "${_val}")
                elseif("${_key}" STREQUAL "app_stack_bytes")
                    set(_app_stack_bytes "${_val}")
                elseif("${_key}" STREQUAL "zenoh_read_priority")
                    set(_zenoh_read_priority "${_val}")
                elseif("${_key}" STREQUAL "zenoh_read_stack_bytes")
                    set(_zenoh_read_stack_bytes "${_val}")
                elseif("${_key}" STREQUAL "zenoh_lease_priority")
                    set(_zenoh_lease_priority "${_val}")
                elseif("${_key}" STREQUAL "zenoh_lease_stack_bytes")
                    set(_zenoh_lease_stack_bytes "${_val}")
                elseif("${_key}" STREQUAL "poll_priority")
                    set(_poll_priority "${_val}")
                elseif("${_key}" STREQUAL "poll_interval_ms")
                    set(_poll_interval_ms "${_val}")
                endif()
            endif()
        endif()
    endforeach()

    set(NROS_CONFIG_IP "${_ip}" PARENT_SCOPE)
    set(NROS_CONFIG_MAC "${_mac}" PARENT_SCOPE)
    set(NROS_CONFIG_GATEWAY "${_gateway}" PARENT_SCOPE)
    set(NROS_CONFIG_NETMASK "${_netmask}" PARENT_SCOPE)
    set(NROS_CONFIG_PREFIX "${_prefix}" PARENT_SCOPE)
    set(NROS_CONFIG_ZENOH_LOCATOR "${_locator}" PARENT_SCOPE)
    set(NROS_CONFIG_DOMAIN_ID "${_domain_id}" PARENT_SCOPE)
    set(NROS_CONFIG_INTERFACE "${_interface}" PARENT_SCOPE)
    set(NROS_CONFIG_APP_PRIORITY "${_app_priority}" PARENT_SCOPE)
    set(NROS_CONFIG_APP_STACK_BYTES "${_app_stack_bytes}" PARENT_SCOPE)
    set(NROS_CONFIG_ZENOH_READ_PRIORITY "${_zenoh_read_priority}" PARENT_SCOPE)
    set(NROS_CONFIG_ZENOH_READ_STACK_BYTES "${_zenoh_read_stack_bytes}" PARENT_SCOPE)
    set(NROS_CONFIG_ZENOH_LEASE_PRIORITY "${_zenoh_lease_priority}" PARENT_SCOPE)
    set(NROS_CONFIG_ZENOH_LEASE_STACK_BYTES "${_zenoh_lease_stack_bytes}" PARENT_SCOPE)
    set(NROS_CONFIG_POLL_PRIORITY "${_poll_priority}" PARENT_SCOPE)
    set(NROS_CONFIG_POLL_INTERVAL_MS "${_poll_interval_ms}" PARENT_SCOPE)
endfunction()

# nano_ros_generate_config_header(<config_file> <out_path>)
#
# Reads <config_file> via nano_ros_read_config(), then emits a typed
# <nros/app_config.h> at <out_path>. User code reads
# `NROS_APP_CONFIG.zenoh.locator` instead of a tree of `APP_*`
# preprocessor macros.
function(nano_ros_generate_config_header CONFIG_FILE OUT_PATH)
    nano_ros_read_config("${CONFIG_FILE}")

    set(NROS_CONFIG_SOURCE "${CONFIG_FILE}")

    set(_NROS_CFG_CMAKE_DIR "${CMAKE_CURRENT_FUNCTION_LIST_DIR}")
    set(_template "${_NROS_CFG_CMAKE_DIR}/templates/nros_app_config.h.in")
    if(NOT EXISTS "${_template}")
        message(FATAL_ERROR
            "nano_ros_generate_config_header: template not found at ${_template}")
    endif()

    configure_file("${_template}" "${OUT_PATH}" @ONLY)
endfunction()

# Convert "192.0.3.10" -> "192,0,3,10"
function(_nros_ip_to_c IP_STR OUT_VAR)
    string(REPLACE "." "," _result "${IP_STR}")
    set(${OUT_VAR} "${_result}" PARENT_SCOPE)
endfunction()

# Convert "02:00:00:00:00:00" -> "0x02,0x00,0x00,0x00,0x00,0x00"
function(_nros_mac_to_c MAC_STR OUT_VAR)
    string(REPLACE ":" ";0x" _result "0x${MAC_STR}")
    string(REPLACE ";" "," _result "${_result}")
    set(${OUT_VAR} "${_result}" PARENT_SCOPE)
endfunction()

# Convert prefix length to dotted netmask: "24" -> "255,255,255,0"
function(_nros_prefix_to_netmask PREFIX OUT_VAR)
    # Common cases
    if("${PREFIX}" STREQUAL "24")
        set(${OUT_VAR} "255,255,255,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "16")
        set(${OUT_VAR} "255,255,0,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "8")
        set(${OUT_VAR} "255,0,0,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "32")
        set(${OUT_VAR} "255,255,255,255" PARENT_SCOPE)
    else()
        # Fallback for less common prefixes
        set(${OUT_VAR} "255,255,255,0" PARENT_SCOPE)
    endif()
endfunction()
