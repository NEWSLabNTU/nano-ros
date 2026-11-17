`nano_ros_hosted_cxx()` makes a C++ node package's own code hosted on a board
whose toolchain compiles C++ freestanding (FreeRTOS), so a library that needs
the hosted standard library, such as Eigen, compiles there. It does nothing
where C++ is already hosted and refuses ThreadX on RISC-V, whose C++ library is
a freestanding shim. The new book page "Bring your own C++ library" shows how to
find and link your own copy of a library without picking up the host's.
