An image's network address moves from flat `[image.<id>] ip` / `gateway` /
`netmask` keys to one entry per interface,
`net = { eth0 = { network = "qemu", host = 50 } }`, and a project can define
its own networks as `[network.<name>]` (`subnet`, `gateway`). The gateway and
netmask now come from the network. The flat keys are refused with a message
naming the new form, as is `net` on a board whose network belongs to the host
OS (`linux`, `freertos-posix`, threadx-linux). An address outside its subnet,
equal to the gateway, or on a network that does not exist fails the build.
A SystemModel transport carrying `ip` / `mac` / `gateway` is refused; those
keys were never read.
