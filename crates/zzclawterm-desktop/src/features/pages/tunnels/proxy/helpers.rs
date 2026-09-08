pub(in crate::features::pages::tunnels) fn proxy_protocol_label(protocol: &str) -> &'static str {
    match protocol {
        "socks5" => "SOCKS5",
        "http" => "HTTP",
        "proxycommand" => "ProxyCommand",
        _ => "Proxy",
    }
}
