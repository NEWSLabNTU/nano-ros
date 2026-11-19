A workspace image whose entry is your own C++ package that depends on message
packages no longer fails while sizing itself: the host build that counts its
entities now takes its board facts from the host board, instead of failing
with "--deploy linux names an image, but this dir has no system.toml".
