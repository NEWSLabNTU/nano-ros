ROS 2 orchestration end-to-end fixture.

This workspace is intentionally small. It provides one launch file, one source
metadata artifact, and one launch manifest so `nros-cli-core` tests can exercise
metadata preservation, launch parsing, planning, checking, generated package
creation, and native Cargo build without requiring a ROS 2 install.
