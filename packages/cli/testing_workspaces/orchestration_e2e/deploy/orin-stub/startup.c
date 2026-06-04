/* Phase 172.V e2e fixture — minimal vendor-lib startup.
 *
 * Stands in for a real vendor startup (e.g. the Orin SPE reset handler). It
 * references one symbol from the (stub) vendor static lib so the link proves
 * `-L{vendor.dir}/lib -lfakevendor` resolves; the generated entry lib
 * (`{entry_lib}`) is linked in alongside. The goal is to exercise the
 * vendor-lib emit→link→package pipeline end-to-end, not to boot a functional
 * image. */
extern int fake_vendor_init(void);

int main(void) {
    return fake_vendor_init();
}
