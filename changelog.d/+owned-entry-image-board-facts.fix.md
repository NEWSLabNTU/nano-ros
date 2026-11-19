A workspace image whose entry is your own CMake package no longer has to be
named after its board: the build now asks for the board facts of the image
itself, instead of failing with "no [image.<board>] naming a board".
