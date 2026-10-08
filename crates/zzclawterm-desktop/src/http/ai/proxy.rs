use zed_reqwest::{NoProxy, Proxy, Url, blocking::ClientBuilder};
use zzclawterm_core::ai::proxy::{AiProxyMode, AiProxyProtocol, AiProxySettings};

pub(super) fn apply_proxy(
    builder: ClientBuilder,
    settings: &AiProxySettings,
) -> Result<ClientBuilder, String> {
    match settings.mode {
        AiProxyMode::System => Ok(builder),
        AiProxyMode::Direct => Ok(builder.no_proxy()),
        AiProxyMode::Custom => {
            let url = proxy_url(settings)?;
            let username = settings
                .username
                .as_deref()
                .filter(|value| !value.is_empty());
            let password = settings
                .password
                .as_deref()
                .filter(|value| !value.is_empty());
            if password.is_some() && username.is_none() {
                return Err("AI proxy password requires a username".to_string());
            }
            if settings.protocol == AiProxyProtocol::Socks5
                && (username.is_some_and(|value| value.len() > 255)
                    || password.is_some_and(|value| value.len() > 255))
            {
                return Err("SOCKS5 proxy credentials must be at most 255 bytes".to_string());
            }
            let mut proxy = Proxy::all(url)
                .map_err(|_| "Invalid AI proxy configuration".to_string())?
                .no_proxy(NoProxy::from_string(&settings.no_proxy));
            if let Some(username) = username {
                proxy = proxy.basic_auth(username, password.unwrap_or_default());
            }
            // Explicitly disable automatic proxies so bypassed requests connect
            // directly, even when environment / system proxies are configured.
            Ok(builder.no_proxy().proxy(proxy))
        }
    }
}

fn proxy_url(settings: &AiProxySettings) -> Result<Url, String> {
    let invalid = || "Invalid AI proxy host or port".to_string();
    let host = settings.host.trim();
    if host.is_empty()
        || settings.port == 0
        || host
            .chars()
            .any(|value| value.is_whitespace() || "/\\@?#%".contains(value))
    {
        return Err(invalid());
    }
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let host = if unbracketed.contains(':') {
        let address = unbracketed
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| invalid())?;
        format!("[{address}]")
    } else {
        host.to_string()
    };
    let scheme = match settings.protocol {
        AiProxyProtocol::Http => "http",
        AiProxyProtocol::Socks5 => "socks5h",
    };
    // Validate DNS names using a special URL scheme too: SOCKS URLs otherwise
    // allow opaque hosts with characters that cannot be used as a DNS name.
    Url::parse(&format!("http://{host}:{}", settings.port)).map_err(|_| invalid())?;
    Url::parse(&format!("{scheme}://{host}:{}", settings.port)).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::{apply_proxy, proxy_url};
    use zed_reqwest::blocking::Client;
    use zzclawterm_core::ai::proxy::{AiProxyMode, AiProxyProtocol, AiProxySettings};

    #[test]
    fn socks_uses_remote_dns_and_rejects_invalid_hosts_without_echoing_them() {
        let mut settings = AiProxySettings {
            mode: AiProxyMode::Custom,
            protocol: AiProxyProtocol::Socks5,
            ..Default::default()
        };
        assert_eq!(proxy_url(&settings).unwrap().scheme(), "socks5h");
        settings.host = "[::1]".into();
        assert_eq!(proxy_url(&settings).unwrap().host_str(), Some("[::1]"));
        settings.host = "user:secret@example.com".into();
        assert_eq!(
            proxy_url(&settings).unwrap_err(),
            "Invalid AI proxy host or port"
        );
        settings.host = "127.0.0.1".into();
        settings.password = Some("fixture".into());
        assert!(apply_proxy(Client::builder(), &settings).is_err());
        settings.username = Some("u".repeat(256));
        assert!(apply_proxy(Client::builder(), &settings).is_err());
    }

    #[test]
    fn custom_bypass_and_direct_reach_the_origin_without_a_proxy() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;
        for mode in [AiProxyMode::Custom, AiProxyMode::Direct] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            let mut request = [0; 2048];
                            assert!(stream.read(&mut request).unwrap() > 0);
                            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
                            return;
                        }
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => panic!("origin did not receive request: {error}"),
                    }
                }
            });
            let settings = AiProxySettings {
                mode,
                port: 1,
                ..Default::default()
            };
            let result = apply_proxy(Client::builder(), &settings)
                .unwrap()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
                .get(format!("http://{address}/"))
                .send();
            server.join().unwrap();
            assert_eq!(result.unwrap().text().unwrap(), "ok");
        }
    }
}
