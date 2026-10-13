C/C++ API: the application writes `int main(int argc, char** argv)`, as an
rclc or rclcpp program does (RFC-0089 amendment 2026-10-13, phase-499 W5).
`nano_ros_add_executable` compiles your sources with `main` renamed and
reaches it from the platform's entry (POSIX `main`, Zephyr `main(void)`, an
RTOS startup task); a cargo-rooted C/C++ image does the same through
`nros_cc_flags::app_main`. Replace `int nros_app_main(int argc, char** argv)`
with `int main(int argc, char** argv)`, delete `NROS_APP_MAIN_REGISTER()` and
the `<nros/app_main.h>` include. In C++ write exactly
`int main(int argc, char** argv)`; another form fails at link naming
`nros_ported_main(int, char**)`. Code that still defines `nros_app_main`
fails to link with a duplicate definition. `ROS2_MAIN` keeps only the hosted
C++ standard library and the std-shaped API.
