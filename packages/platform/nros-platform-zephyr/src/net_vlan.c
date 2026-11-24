/*
 * The VLAN interface of a board network file (`vlan = <tag>` on its
 * interface), brought up at boot.
 *
 * `nros build` renders the interface's address into Zephyr's own
 * NET_CONFIG_MY_IPV4_* / NET_DHCPV4 rows, turns NET_CONFIG_AUTO_INIT off and
 * sets CONFIG_NROS_NET_VLAN_TAG. Zephyr's VLAN is a virtual interface attached
 * to the Ethernet one, and its boot-time net_config (NET_CONFIG_MY_VLAN_ID)
 * enables the tag and then adds the address, or starts DHCPv4, on the
 * untagged Ethernet interface (subsys/net/lib/config/init.c, 3.7.2 and
 * 4.4.0). So this file does what that init does, on the VLAN interface:
 *
 *   1. tag the first Ethernet interface (net_eth_vlan_enable);
 *   2. make the VLAN interface the default, so the RMW's sockets, the
 *      network wait (net_wait.c) and off-link routes use it;
 *   3. bring it up and hand it to Zephyr's own net_config_init_by_iface,
 *      which applies the same NET_CONFIG_MY_IPV4_* values or starts DHCPv4,
 *      and waits for the IPv4 address up to NET_CONFIG_INIT_TIMEOUT, as the
 *      untagged image's NET_CONFIG_NEED_IPV4 does.
 *
 * It runs at the APPLICATION init level, where Zephyr's own init_app runs,
 * so the application starts with the address in place.
 */

#include <zephyr/init.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net/ethernet.h>
#include <zephyr/net/net_config.h>
#include <zephyr/net/net_if.h>

#if !defined(CONFIG_NET_VLAN)
#error "NROS_NET_VLAN_TAG needs NET_VLAN=y (the rendered network fragment sets it)"
#endif
#if !defined(CONFIG_NET_CONFIG_SETTINGS)
#error "NROS_NET_VLAN_TAG needs NET_CONFIG_SETTINGS=y (the rendered fragment sets it)"
#endif
#if defined(CONFIG_NET_CONFIG_AUTO_INIT)
/* Zephyr's boot-time net_config would address the untagged interface. */
#error "NROS_NET_VLAN_TAG needs NET_CONFIG_AUTO_INIT=n (the rendered fragment sets it)"
#endif
#if defined(CONFIG_NET_CONFIG_MY_VLAN_ID) && CONFIG_NET_CONFIG_MY_VLAN_ID > 0
#error "NROS_NET_VLAN_TAG and NET_CONFIG_MY_VLAN_ID both tag: state it in the board network file"
#endif

LOG_MODULE_REGISTER(nros_net_vlan, LOG_LEVEL_INF);

static int nros_net_vlan_init(void) {
    const uint16_t tag = CONFIG_NROS_NET_VLAN_TAG;
    struct net_if* eth = net_if_get_first_by_type(&NET_L2_GET_NAME(ETHERNET));

    if (eth == NULL) {
        LOG_ERR("VLAN %u: no Ethernet interface to tag", tag);
        return 0;
    }

    int ret = net_eth_vlan_enable(eth, tag);
    if (ret < 0 && ret != -EALREADY) {
        LOG_ERR("VLAN %u: cannot tag interface %d (%d)", tag, net_if_get_by_iface(eth), ret);
        return 0;
    }

    struct net_if* vlan = net_eth_get_vlan_iface(eth, tag);
    if (vlan == NULL || vlan == eth) {
        LOG_ERR("VLAN %u: no VLAN interface attached to interface %d", tag,
                net_if_get_by_iface(eth));
        return 0;
    }

    /* A VLAN interface is created NO_AUTO_START, and net_config_init_by_iface
     * refuses such an interface; it is started here instead. */
    net_if_flag_clear(vlan, NET_IF_NO_AUTO_START);
    net_if_set_default(vlan);
    (void)net_if_up(vlan);

    ret = net_config_init_by_iface(vlan, NULL, NET_CONFIG_NEED_IPV4,
                                   CONFIG_NET_CONFIG_INIT_TIMEOUT * MSEC_PER_SEC);
    if (ret < 0) {
        LOG_ERR("VLAN %u: interface %d not configured (%d)", tag, net_if_get_by_iface(vlan), ret);
    } else {
        LOG_INF("VLAN %u: interface %d on interface %d configured", tag, net_if_get_by_iface(vlan),
                net_if_get_by_iface(eth));
    }
    return 0;
}

SYS_INIT(nros_net_vlan_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
