# W4.c preamble — what the builder cannot derive for THIS workspace.
#
# `features/` is the one workspace that defines its own message packages and
# consumes them from C/C++ node packages in the same tree. The rclcpp compat
# layer's `Find<pkg>.cmake` stubs search `NROS_INTERFACE_SEARCH_PATH` before
# `AMENT_PREFIX_PATH` and before the bundled set, so without this the custom-msg
# entries resolve the AMENT copy of a message name that also exists there, or
# fail to resolve at all.
#
# It is a preamble rather than a generated line because nothing in
# `[image.*]` implies it: the builder knows the images, not that this
# workspace's `src/` is also an interface search root. That is precisely the
# split RFC-0065 D5 draws — derived facts are generated, authored ones are
# authored — and the same slot `autoware-safety-island` uses for its
# `find_package(Eigen3 REQUIRED)`.
#
# `CMAKE_CURRENT_LIST_DIR`, not `CMAKE_CURRENT_SOURCE_DIR`: inside an
# `include()` the latter is the INCLUDING file's directory, which is the
# generated root under `build/<coord>/`. Resolving from this file's own
# location keeps the answer right wherever the builder puts that root.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_LIST_DIR}/../..")
