# NanoRosReadConfig.cmake
#
# Provides nano_ros_read_config() for reading network/zenoh config.toml files.
# Included automatically by NanoRosConfig.cmake.
#
# Usage:
#   nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
#   # Sets: NROS_CONFIG_IP, NROS_CONFIG_MAC, NROS_CONFIG_GATEWAY,
#   #       NROS_CONFIG_NETMASK, NROS_CONFIG_PREFIX,
#   #       NROS_CONFIG_ZENOH_LOCATOR, NROS_CONFIG_DOMAIN_ID

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
#   NROS_CONFIG_INTERFACE     - e.g. "veth-tx0" (optional, from [platform] section)
#
function(nano_ros_read_config CONFIG_FILE)
    if(NOT EXISTS "${CONFIG_FILE}")
        message(FATAL_ERROR "nano_ros_read_config: ${CONFIG_FILE} not found")
    endif()

    file(READ "${CONFIG_FILE}" _content)

    # Defaults
    set(_ip "192,0,3,10")
    set(_mac "0x02,0x00,0x00,0x00,0x00,0x00")
    set(_gateway "192,0,3,1")
    set(_netmask "255,255,255,0")
    set(_prefix "24")
    set(_locator "tcp/127.0.0.1:7447")
    set(_domain_id "0")
    set(_interface "")

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
                    # Also derive netmask from prefix
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
    if("${PREFIX}" STREQUAL "24")
        set(${OUT_VAR} "255,255,255,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "16")
        set(${OUT_VAR} "255,255,0,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "8")
        set(${OUT_VAR} "255,0,0,0" PARENT_SCOPE)
    elseif("${PREFIX}" STREQUAL "32")
        set(${OUT_VAR} "255,255,255,255" PARENT_SCOPE)
    else()
        set(${OUT_VAR} "255,255,255,0" PARENT_SCOPE)
    endif()
endfunction()
