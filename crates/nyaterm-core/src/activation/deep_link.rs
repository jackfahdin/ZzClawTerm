use std::net::IpAddr;
use std::str::FromStr as _;

use percent_encoding::percent_decode_str;
use thiserror::Error;
use url::{Host, Url};

use super::{ActivationRequest, RawActivationArg};

pub const MAX_DEEP_LINK_BYTES: usize = 4 * 1024;
pub const MAX_ACTIVATION_ACTIONS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivationAction {
    Activate,
    Connect(ExternalConnectionRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalConnectionRequest {
    Ssh {
        host: String,
        port: u16,
        username: Option<String>,
    },
    Telnet {
        host: String,
        port: u16,
    },
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ActivationParseError {
    #[error("too many activation arguments")]
    TooManyActions,
    #[error("activation argument is not valid text")]
    InvalidTextEncoding,
    #[error("activation argument is empty or contains unsafe text")]
    InvalidText,
    #[error("activation link exceeds the size limit")]
    LinkTooLong,
    #[error("activation link is not a valid URL")]
    InvalidUrl,
    #[error("activation link uses an unsupported scheme")]
    UnsupportedScheme,
    #[error("activation link contains credentials")]
    CredentialsNotAllowed,
    #[error("activation link contains an unsupported URL component")]
    UnsupportedComponent,
    #[error("activation link targets an unsupported action")]
    UnsupportedAction,
    #[error("activation link is missing a valid host")]
    InvalidHost,
    #[error("activation link contains an invalid username")]
    InvalidUsername,
    #[error("activation link contains an invalid port")]
    InvalidPort,
    #[error("activation link contains an invalid query parameter")]
    InvalidQuery,
    #[error("activation link repeats a query parameter")]
    DuplicateParameter,
    #[error("activation link contains an unknown query parameter")]
    UnknownParameter,
}

pub fn parse_activation_request(
    request: &ActivationRequest,
) -> Result<Vec<ActivationAction>, ActivationParseError> {
    if request.args.is_empty() {
        return Ok(vec![ActivationAction::Activate]);
    }
    if request.args.len() > MAX_ACTIVATION_ACTIONS {
        return Err(ActivationParseError::TooManyActions);
    }
    request.args.iter().map(parse_activation_arg).collect()
}

fn parse_activation_arg(arg: &RawActivationArg) -> Result<ActivationAction, ActivationParseError> {
    let text = match arg {
        RawActivationArg::Bytes(bytes) => String::from_utf8(bytes.clone())
            .map_err(|_| ActivationParseError::InvalidTextEncoding)?,
        RawActivationArg::Wide(units) => {
            String::from_utf16(units).map_err(|_| ActivationParseError::InvalidTextEncoding)?
        }
    };
    if text.len() > MAX_DEEP_LINK_BYTES {
        return Err(ActivationParseError::LinkTooLong);
    }
    if text.is_empty() || text.trim() != text || text.chars().any(char::is_control) {
        return Err(ActivationParseError::InvalidText);
    }
    parse_deep_link(&text)
}

pub fn parse_deep_link(text: &str) -> Result<ActivationAction, ActivationParseError> {
    if text.len() > MAX_DEEP_LINK_BYTES {
        return Err(ActivationParseError::LinkTooLong);
    }
    if text.is_empty() || text.trim() != text || text.chars().any(char::is_control) {
        return Err(ActivationParseError::InvalidText);
    }
    let url = Url::parse(text).map_err(|_| ActivationParseError::InvalidUrl)?;
    match url.scheme() {
        "ssh" if text.starts_with("ssh://") => parse_ssh_url(text, &url),
        "telnet" if text.starts_with("telnet://") => parse_telnet_url(text, &url),
        "nyaterm" if text.starts_with("nyaterm://") => parse_nyaterm_url(&url),
        _ => Err(ActivationParseError::UnsupportedScheme),
    }
}

fn parse_ssh_url(text: &str, url: &Url) -> Result<ActivationAction, ActivationParseError> {
    reject_common_suffix(url)?;
    if !url.path().is_empty() || url.query().is_some() {
        return Err(ActivationParseError::UnsupportedComponent);
    }
    let authority = authority(text, "ssh://")?;
    if authority.ends_with(':') {
        return Err(ActivationParseError::InvalidPort);
    }
    if url.password().is_some() {
        return Err(ActivationParseError::CredentialsNotAllowed);
    }
    let username = if authority.contains('@') {
        Some(validate_url_username(url.username())?)
    } else {
        None
    };
    let host = normalize_url_host(url)?;
    let port = url_port(url, 22)?;
    Ok(ActivationAction::Connect(ExternalConnectionRequest::Ssh {
        host,
        port,
        username,
    }))
}

fn parse_telnet_url(text: &str, url: &Url) -> Result<ActivationAction, ActivationParseError> {
    reject_common_suffix(url)?;
    if !url.path().is_empty() || url.query().is_some() {
        return Err(ActivationParseError::UnsupportedComponent);
    }
    let authority = authority(text, "telnet://")?;
    if authority.ends_with(':') {
        return Err(ActivationParseError::InvalidPort);
    }
    if authority.contains('@') || !url.username().is_empty() || url.password().is_some() {
        return Err(ActivationParseError::CredentialsNotAllowed);
    }
    let host = normalize_url_host(url)?;
    let port = url_port(url, 23)?;
    Ok(ActivationAction::Connect(
        ExternalConnectionRequest::Telnet { host, port },
    ))
}

fn parse_nyaterm_url(url: &Url) -> Result<ActivationAction, ActivationParseError> {
    reject_common_suffix(url)?;
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err(ActivationParseError::CredentialsNotAllowed);
    }
    if url.host_str() != Some("connect") {
        return Err(ActivationParseError::UnsupportedAction);
    }
    match url.path() {
        "/ssh" => parse_nyaterm_ssh_query(url.query()),
        "/telnet" => parse_nyaterm_telnet_query(url.query()),
        _ => Err(ActivationParseError::UnsupportedAction),
    }
}

fn parse_nyaterm_ssh_query(query: Option<&str>) -> Result<ActivationAction, ActivationParseError> {
    let mut host = None;
    let mut port = None;
    let mut username = None;
    for (key, value) in strict_query_pairs(query)? {
        match key.as_str() {
            "host" => set_once(&mut host, normalize_query_host(&value)?)?,
            "port" => set_once(&mut port, parse_port(&value)?)?,
            "username" => set_once(&mut username, validate_query_username(value)?)?,
            _ => return Err(ActivationParseError::UnknownParameter),
        }
    }
    Ok(ActivationAction::Connect(ExternalConnectionRequest::Ssh {
        host: host.ok_or(ActivationParseError::InvalidHost)?,
        port: port.unwrap_or(22),
        username,
    }))
}

fn parse_nyaterm_telnet_query(
    query: Option<&str>,
) -> Result<ActivationAction, ActivationParseError> {
    let mut host = None;
    let mut port = None;
    for (key, value) in strict_query_pairs(query)? {
        match key.as_str() {
            "host" => set_once(&mut host, normalize_query_host(&value)?)?,
            "port" => set_once(&mut port, parse_port(&value)?)?,
            _ => return Err(ActivationParseError::UnknownParameter),
        }
    }
    Ok(ActivationAction::Connect(
        ExternalConnectionRequest::Telnet {
            host: host.ok_or(ActivationParseError::InvalidHost)?,
            port: port.unwrap_or(23),
        },
    ))
}

fn reject_common_suffix(url: &Url) -> Result<(), ActivationParseError> {
    if url.fragment().is_some() {
        return Err(ActivationParseError::UnsupportedComponent);
    }
    Ok(())
}

fn authority<'a>(text: &'a str, prefix: &str) -> Result<&'a str, ActivationParseError> {
    let rest = text
        .strip_prefix(prefix)
        .ok_or(ActivationParseError::UnsupportedScheme)?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() {
        return Err(ActivationParseError::InvalidHost);
    }
    Ok(authority)
}

fn normalize_url_host(url: &Url) -> Result<String, ActivationParseError> {
    let host = url.host().ok_or(ActivationParseError::InvalidHost)?;
    normalize_host(host)
}

fn normalize_query_host(value: &str) -> Result<String, ActivationParseError> {
    validate_plain_text(value, 253).map_err(|_| ActivationParseError::InvalidHost)?;
    if value.contains(['/', '\\', '@', '?', '#']) {
        return Err(ActivationParseError::InvalidHost);
    }
    if let Ok(address) = IpAddr::from_str(value) {
        return Ok(address.to_string());
    }
    normalize_host(Host::parse(value).map_err(|_| ActivationParseError::InvalidHost)?)
}

fn normalize_host<S: AsRef<str>>(host: Host<S>) -> Result<String, ActivationParseError> {
    let value = match host {
        Host::Domain(domain) => domain.as_ref().to_string(),
        Host::Ipv4(address) => address.to_string(),
        Host::Ipv6(address) => address.to_string(),
    };
    validate_plain_text(&value, 253).map_err(|_| ActivationParseError::InvalidHost)?;
    Ok(value)
}

fn validate_url_username(value: &str) -> Result<String, ActivationParseError> {
    if value.contains('%') {
        return Err(ActivationParseError::InvalidUsername);
    }
    validate_query_username(value.to_string())
}

fn validate_query_username(value: String) -> Result<String, ActivationParseError> {
    validate_plain_text(&value, 128).map_err(|_| ActivationParseError::InvalidUsername)?;
    if value.contains(['@', ':', '/', '\\', '?', '#']) {
        return Err(ActivationParseError::InvalidUsername);
    }
    Ok(value)
}

fn validate_plain_text(value: &str, max_bytes: usize) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(());
    }
    Ok(())
}

fn url_port(url: &Url, default: u16) -> Result<u16, ActivationParseError> {
    match url.port() {
        Some(0) => Err(ActivationParseError::InvalidPort),
        Some(port) => Ok(port),
        None => Ok(default),
    }
}

fn parse_port(value: &str) -> Result<u16, ActivationParseError> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(ActivationParseError::InvalidPort)
}

fn strict_query_pairs(query: Option<&str>) -> Result<Vec<(String, String)>, ActivationParseError> {
    let query = query.ok_or(ActivationParseError::InvalidQuery)?;
    if query.is_empty() {
        return Err(ActivationParseError::InvalidQuery);
    }
    query
        .split('&')
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .ok_or(ActivationParseError::InvalidQuery)?;
            if key.is_empty() || value.is_empty() || value.contains('+') {
                return Err(ActivationParseError::InvalidQuery);
            }
            Ok((strict_percent_decode(key)?, strict_percent_decode(value)?))
        })
        .collect()
}

fn strict_percent_decode(value: &str) -> Result<String, ActivationParseError> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ActivationParseError::InvalidQuery);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    let decoded = percent_decode_str(value)
        .decode_utf8()
        .map_err(|_| ActivationParseError::InvalidQuery)?
        .into_owned();
    if decoded.chars().any(char::is_control) {
        return Err(ActivationParseError::InvalidQuery);
    }
    Ok(decoded)
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), ActivationParseError> {
    if slot.replace(value).is_some() {
        return Err(ActivationParseError::DuplicateParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationAction, ActivationParseError, ExternalConnectionRequest, MAX_ACTIVATION_ACTIONS,
        parse_activation_request, parse_deep_link,
    };
    use crate::{ActivationRequest, RawActivationArg};

    #[test]
    fn empty_request_activates_without_connecting() {
        let request = ActivationRequest {
            request_id: [1; 16],
            args: Vec::new(),
        };
        assert_eq!(
            parse_activation_request(&request),
            Ok(vec![ActivationAction::Activate])
        );
    }

    #[test]
    fn parses_strict_ssh_telnet_and_nyaterm_links() {
        assert_eq!(
            parse_deep_link("ssh://alice@example.com:2200"),
            Ok(ActivationAction::Connect(ExternalConnectionRequest::Ssh {
                host: "example.com".to_string(),
                port: 2200,
                username: Some("alice".to_string()),
            }))
        );
        assert_eq!(
            parse_deep_link("telnet://[2001:db8::1]"),
            Ok(ActivationAction::Connect(
                ExternalConnectionRequest::Telnet {
                    host: "2001:db8::1".to_string(),
                    port: 23,
                }
            ))
        );
        assert_eq!(
            parse_deep_link("nyaterm://connect/ssh?host=example.com&port=22&username=deploy"),
            Ok(ActivationAction::Connect(ExternalConnectionRequest::Ssh {
                host: "example.com".to_string(),
                port: 22,
                username: Some("deploy".to_string()),
            }))
        );
        assert_eq!(
            parse_deep_link("nyaterm://connect/telnet?host=example.com"),
            Ok(ActivationAction::Connect(
                ExternalConnectionRequest::Telnet {
                    host: "example.com".to_string(),
                    port: 23,
                }
            ))
        );
    }

    #[test]
    fn raw_byte_and_wide_arguments_share_the_same_parser() {
        let expected = ActivationAction::Connect(ExternalConnectionRequest::Ssh {
            host: "example.com".to_string(),
            port: 22,
            username: None,
        });
        for arg in [
            RawActivationArg::Bytes(b"ssh://example.com".to_vec()),
            RawActivationArg::Wide("ssh://example.com".encode_utf16().collect()),
        ] {
            let request = ActivationRequest {
                request_id: [2; 16],
                args: vec![arg],
            };
            assert_eq!(
                parse_activation_request(&request),
                Ok(vec![expected.clone()])
            );
        }
    }

    #[test]
    fn rejects_credentials_suffixes_and_noncanonical_schemes() {
        for link in [
            "ssh://alice:secret@example.com",
            "telnet://alice@example.com",
            "ssh://example.com/command",
            "ssh://example.com?command=id",
            "ssh://example.com:0",
            "ssh://example.com:",
            "telnet://example.com:0",
            "telnet://example.com#fragment",
            "SSH://example.com",
            "https://example.com",
            "nyaterm://user@connect/ssh?host=example.com",
        ] {
            assert!(parse_deep_link(link).is_err(), "accepted {link}");
        }
    }

    #[test]
    fn rejects_unknown_duplicate_and_unsafe_query_values() {
        assert_eq!(
            parse_deep_link("nyaterm://connect/ssh?host=a&host=b"),
            Err(ActivationParseError::DuplicateParameter)
        );
        for link in [
            "nyaterm://connect/ssh?host=a&password=secret",
            "nyaterm://connect/ssh?host=a&command=id",
            "nyaterm://connect/ssh?host=%00example.com",
            "nyaterm://connect/ssh?host=%GG",
            "nyaterm://connect/ssh?host=example.com%2Fother",
            "nyaterm://connect/rdp?host=example.com",
            "nyaterm://connect/telnet?host=a&username=user",
        ] {
            assert!(parse_deep_link(link).is_err(), "accepted {link}");
        }
    }

    #[test]
    fn rejects_invalid_os_text_and_action_floods() {
        let invalid_bytes = ActivationRequest {
            request_id: [3; 16],
            args: vec![RawActivationArg::Bytes(vec![0xff])],
        };
        assert_eq!(
            parse_activation_request(&invalid_bytes),
            Err(ActivationParseError::InvalidTextEncoding)
        );
        let invalid_wide = ActivationRequest {
            request_id: [4; 16],
            args: vec![RawActivationArg::Wide(vec![0xd800])],
        };
        assert_eq!(
            parse_activation_request(&invalid_wide),
            Err(ActivationParseError::InvalidTextEncoding)
        );
        let flooded = ActivationRequest {
            request_id: [5; 16],
            args: vec![
                RawActivationArg::Bytes(b"ssh://example.com".to_vec());
                MAX_ACTIVATION_ACTIONS + 1
            ],
        };
        assert_eq!(
            parse_activation_request(&flooded),
            Err(ActivationParseError::TooManyActions)
        );
    }
}
