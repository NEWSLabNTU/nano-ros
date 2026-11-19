`nros run` now runs an image whose entry is your own CMake package: it asks
CMake for the executable that package defines, instead of stopping with "the
plan carries no `elf` artifact to run". A package that defines several
executables is refused, naming them, since an image runs one.
