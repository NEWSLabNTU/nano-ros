/* Zephyr's `app` library must name a source, or the configure fails at
 * GENERATE ("No SOURCES given to target: app", issue 1536) -- after
 * `nros_system_generate` has already baked, which is why the row's declared
 * output hid it. This row is `west-configure`: the file is never compiled. */
int main(void) {
    return 0;
}
