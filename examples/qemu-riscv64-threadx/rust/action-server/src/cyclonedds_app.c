/* Empty translation unit for the CMake/CycloneDDS link path.
 * The real application entry is the Rust staticlib's app_main().
 */
void qemu_riscv64_threadx_action_server_cyclonedds_link_anchor(void) {}

/* CycloneDDS app-descriptor registration (phase-245 W2 cyclone tail).
 * Strong-overrides the weak nros_rmw_cyclonedds_register_app_descriptors
 * (packages/dds/nros-rmw-cyclonedds/src/vtable.cpp) so the Fibonacci type is
 * registered with CycloneDDS before publish/subscribe. register_* come from the
 * idlc-generated *__cyclonedds_ts (linked via nano_ros_link_rmw cyclonedds). */
extern void register_example_interfaces_Fibonacci_0(void);
extern void register_example_interfaces_Fibonacci_1(void);
extern void register_example_interfaces_Fibonacci_2(void);
extern void register_example_interfaces_Fibonacci_3(void);
extern void register_example_interfaces_Fibonacci_4(void);
extern void register_example_interfaces_Fibonacci_5(void);
extern void register_example_interfaces_Fibonacci_6(void);
extern void register_example_interfaces_Fibonacci_7(void);

void nros_rmw_cyclonedds_register_app_descriptors(void) {
    register_example_interfaces_Fibonacci_0();
    register_example_interfaces_Fibonacci_1();
    register_example_interfaces_Fibonacci_2();
    register_example_interfaces_Fibonacci_3();
    register_example_interfaces_Fibonacci_4();
    register_example_interfaces_Fibonacci_5();
    register_example_interfaces_Fibonacci_6();
    register_example_interfaces_Fibonacci_7();
}
