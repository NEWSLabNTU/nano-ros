A Zephyr source tree without `.west/` (such as CI's sparse clone of the C++
minimal-libc headers) no longer resolves as a west workspace, in the shell
resolver or in `nros build`'s ladder. Gates that need a buildable workspace now
report "Zephyr not provisioned" over such a tree, instead of running west
against it and failing.
