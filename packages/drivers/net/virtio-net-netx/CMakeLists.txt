cmake_minimum_required(VERSION 3.22)
project(virtio-net-netx LANGUAGES C)

# NETX_DIR is required -- consumer must set it (or inherit from env)
if(NOT DEFINED NETX_DIR)
    if(DEFINED ENV{NETX_DIR})
        set(NETX_DIR $ENV{NETX_DIR})
    else()
        message(FATAL_ERROR
            "NETX_DIR not set. Point it at the NetX Duo source directory.\n"
            "Run `just setup-threadx` to download, then:\n"
            "  cmake -DNETX_DIR=<repo>/third-party/threadx/netxduo ..")
    endif()
endif()

add_library(virtio_net_netx STATIC
    src/virtio_mmio.c
    src/virtqueue.c
    src/virtio_net_nx.c
)

target_include_directories(virtio_net_netx
    PUBLIC  include                              # virtio_net_nx.h
    PRIVATE src                                  # virtio_mmio.h, virtqueue.h
    PRIVATE ${NETX_DIR}/common/inc               # nx_api.h
)

# ThreadX headers and PLIC headers are board-specific.
# Consumer must provide them, e.g.:
#   target_include_directories(virtio_net_netx PRIVATE ${THREADX_DIR}/common/inc)
#   target_include_directories(virtio_net_netx PRIVATE ${THREADX_PORT_DIR}/inc)
#   target_include_directories(virtio_net_netx PRIVATE ${PLIC_DIR})
