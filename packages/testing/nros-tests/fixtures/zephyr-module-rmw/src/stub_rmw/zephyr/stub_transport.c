/* The `stub` RMW package's transport (phase-581 W8): a stub. It carries
 * nothing; it exists so the package's own Zephyr module compiles a source of
 * its own when the image selects CONFIG_NROS_RMW_STUB. */
int stub_rmw_transport_ready(void)
{
    return 0;
}
