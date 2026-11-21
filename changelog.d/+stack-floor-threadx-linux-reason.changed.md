`nros build` now explains why it does not check the stack of a ThreadX image built for Linux: that port runs every thread on a host pthread stack, which the build does not size.
