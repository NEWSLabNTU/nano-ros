Fixture and image builds at the default `nros-relwithdebinfo` profile no longer
put debuginfo in build scripts and proc-macros (`build-override.debug = 0`). On
a native C talker, the host half of the leaf's cargo directory shrinks from 842
to 375 MiB. The image, and the C objects a build script compiles for it, keep
their debuginfo. A CMake build directory now picks up a changed cargo profile
definition on its next configure; it used to keep the definition from its first
configure.
