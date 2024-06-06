pub fn configure_iptables(interface_name: &str, portal_addr: &str) {
    let ipt = iptables::new(false).expect("Failed to initialize iptables");

    // Redirect HTTP traffic
    ipt.append(
        "nat",
        "PREROUTING",
        format!("-p tcp --dport 80 -j DNAT --to-destination {}", portal_addr).as_str(),
    )
    .expect("Failed to append iptables rule");

    // Redirect HTTPS traffic
    ipt.append(
        "nat",
        "PREROUTING",
        format!(
            "-p tcp --dport 443 -j DNAT --to-destination {}",
            portal_addr
        )
        .as_str(),
    )
    .expect("Failed to append iptables rule");

    // Masquerade outgoing traffic
    ipt.append("nat", "POSTROUTING", "-j MASQUERADE")
        .expect("Failed to append iptables rule");
}
