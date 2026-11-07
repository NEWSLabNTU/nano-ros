# Security policy

## Reporting a vulnerability

Report it privately through GitHub:
<https://github.com/NEWSLabNTU/nano-ros/security/advisories/new>.
Do not open a public issue or pull request for it.

Include the release (`nros version`, or the `version` in your project's
`nros-toolchain.toml`), the board and RMW, and the smallest reproduction you
have. We acknowledge a report within seven days and keep you informed while we
work on it.

## What is supported

Only the newest release. nano-ros is pre-1.0, there is no long-term-support
line, and fixes are not backported: a security fix ships in the next release,
and you take it by moving your pin. A release reaches end of life when the
next one is published. Its tag and assets stay published, so a pinned project
still installs the same bytes.

## Advisories

A fixed vulnerability is published as a GitHub Security Advisory on this
repository, naming the affected releases and the release that fixes it.

## What a release declares

Each release publishes `nros-linux-x86_64.cdx.json` beside its asset, and
installs the same file as `share/nros/sbom.cdx.json`. It is a CycloneDX SBOM
of every crate compiled into the toolchain's binaries and prebuilt archives.
What a firmware image is built from (tools, sources, ROS 2 packages) is
recorded per project in `nros-sdk.lock`, not in the SBOM.

Vulnerabilities in vendored third-party code (zenoh-pico, Cyclone DDS,
Micro XRCE-DDS, Mbed TLS, the RTOS kernels) are also in scope when nano-ros
ships or pins the affected version.
