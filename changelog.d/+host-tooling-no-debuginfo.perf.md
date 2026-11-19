Fixture and image builds at the default `nros-relwithdebinfo` profile no longer
put debuginfo in build scripts and proc-macros (`build-override.debug = 0`). The
host half of each leaf's cargo directory shrinks by roughly 3x, and the image
and the C objects a build script compiles for it keep their debuginfo.
