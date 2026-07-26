# syntax=docker/dockerfile:1
#
# nano-ros multi-edition ROS test image (RFC-0058 / phase-309) — INTERNAL TEST
# INFRASTRUCTURE ONLY. Never part of the nano-ros ship path; users build against
# their own host ROS. This image lets ONE host exercise codegen + interop against
# a ROS 2 edition it does not (and cannot) have installed natively.
#
# Parametric over the distro:
#   docker build --build-arg DISTRO=jazzy -t nano-ros-ros:jazzy docker/ros-editions
# (the `just ros-editions image <distro>` recipe wraps this).
#
# Contents: the edition's ROS base + the middlewares nano-ros interoperates with
# (cyclonedds — the #0267 reference; fastrtps; rmw_zenoh where the distro ships
# it in apt), `domain_bridge`, the interface packages the interop lanes use
# (example_interfaces, geometry_msgs, std_msgs), rclpy for scripted peers, and
# the rosidl codegen toolchain.
#
# NOTE on the zenoh pin: RFC-0058 calls for the pinned zenoh 1.7.2 overlay to
# wire-match `packages/zpico`. For the cyclone-based reference lane that pin is
# irrelevant, so this baseline installs apt `rmw_zenoh_cpp` where available and
# leaves the source-built pin as a future opt-in layer (ARG WITH_ZENOH_PIN),
# keeping the image an apt-only build (minutes, no source compile).

ARG DISTRO=jazzy
FROM ros:${DISTRO}-ros-base
ARG DISTRO
ENV DEBIAN_FRONTEND=noninteractive

# Middlewares + bridge + interface packages + rclpy + codegen toolchain.
# rmw_zenoh_cpp is best-effort: humble has no apt package (needs the source
# overlay), iron/jazzy+ do — so a failed install there is not fatal.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ros-${DISTRO}-rmw-cyclonedds-cpp \
        ros-${DISTRO}-rmw-fastrtps-cpp \
        ros-${DISTRO}-domain-bridge \
        ros-${DISTRO}-example-interfaces \
        ros-${DISTRO}-geometry-msgs \
        ros-${DISTRO}-std-msgs \
        ros-${DISTRO}-rosidl-adapter \
        ros-${DISTRO}-rclpy \
    && (apt-get install -y --no-install-recommends ros-${DISTRO}-rmw-zenoh-cpp \
        || echo "note: rmw_zenoh_cpp not in apt for ${DISTRO} (source overlay needed)") \
    && rm -rf /var/lib/apt/lists/*

LABEL org.nano-ros.image="ros-edition" \
      org.nano-ros.distro="${DISTRO}"

# The container's default entrypoint sources the distro; test commands source it
# again explicitly (RosEnv::shell), so a bare `bash -lc` also works.
