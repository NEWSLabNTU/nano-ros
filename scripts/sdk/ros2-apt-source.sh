#!/usr/bin/env bash
# Add the ROS 2 apt repository (packages.ros.org/ros2) to an Ubuntu host.
#
# ONE spelling of the repository, for every place that needs it: the
# self-hosted runner IMAGE (`scripts/ci/runner-container.sh` copies this into
# its build context and runs it as the image's root, issue 1695) and the
# distrobox setup (`scripts/dev/ros2-distrobox-setup.sh`, run with the box's
# sudo). Two hand-written copies of a signing-key URL and a `deb` line are two
# things to rotate when the key rotates -- and ROS rotated it in 2025.
#
# Run as root. Idempotent: it rewrites the same two files every time.
# Needs `curl` and `ca-certificates`, nothing else. The key ros/rosdistro
# publishes is a BINARY OpenPGP key (measured: `gpg --show-keys` reads it,
# `C1CF6E31...F42ED6FBAB17C654`, expires 2030-06-01), which apt takes as-is from
# a `.gpg` keyring -- so no `gpg --dearmor` and no gnupg. Saved as `.asc` it
# fails with NO_PUBKEY: apt reads that extension as ARMORED.
#
# Installs no package. What to install from the repository is the caller's
# business, resolved from `nros-sdk-index.toml` (`prereq-packages.py`), never a
# list written here.
set -euo pipefail

if [ "$(id -u)" != "0" ]; then
    echo "ros2-apt-source: must run as root (it writes /etc/apt/sources.list.d)." >&2
    exit 1
fi

# shellcheck disable=SC1091
. /etc/os-release
codename="${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}"
if [ -z "$codename" ]; then
    echo "ros2-apt-source: /etc/os-release names no Ubuntu codename; packages.ros.org" >&2
    echo "  publishes per Ubuntu release, so there is no repository line to write." >&2
    exit 1
fi

keyring=/usr/share/keyrings/ros-archive-keyring.gpg
install -d -m 0755 /usr/share/keyrings
curl -fsSL https://raw.githubusercontent.com/ros/rosdistro/master/ros.key -o "$keyring"
chmod 0644 "$keyring"
echo "deb [arch=$(dpkg --print-architecture) signed-by=$keyring] http://packages.ros.org/ros2/ubuntu $codename main" \
    > /etc/apt/sources.list.d/ros2.list
apt-get update -qq
