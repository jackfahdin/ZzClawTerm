//! Local IPv4 listening endpoints. Queries run only on the server monitor.

use std::net::Ipv4Addr;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(super) use windows::query;

fn listening_displays(
    bindings: &[Ipv4Addr],
    adapters: &[(Ipv4Addr, String)],
    display: u16,
) -> Vec<String> {
    let mut result = Vec::new();
    for binding in bindings {
        if binding.is_unspecified() {
            for (address, name) in adapters {
                result.push(format!("{address}:{display}.0 ({name})"));
            }
        } else {
            let name = adapters.iter().find(|(address, _)| address == binding);
            result.push(match name {
                Some((_, name)) => format!("{binding}:{display}.0 ({name})"),
                None => format!("{binding}:{display}.0"),
            });
        }
    }
    result.sort();
    result.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::listening_displays;
    use std::net::Ipv4Addr;

    #[test]
    fn loopback_listener_does_not_advertise_lan_interfaces() {
        let adapters = vec![
            (Ipv4Addr::LOCALHOST, "Loopback".into()),
            (Ipv4Addr::new(192, 168, 1, 2), "Ethernet".into()),
        ];
        assert_eq!(
            listening_displays(&[Ipv4Addr::LOCALHOST], &adapters, 1),
            vec!["127.0.0.1:1.0 (Loopback)"]
        );
        assert_eq!(
            listening_displays(&[Ipv4Addr::UNSPECIFIED], &adapters, 2),
            vec!["127.0.0.1:2.0 (Loopback)", "192.168.1.2:2.0 (Ethernet)"]
        );
        assert!(listening_displays(&[], &adapters, 0).is_empty());
    }
}
