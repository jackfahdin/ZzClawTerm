use std::collections::HashMap;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use super::{AppError, AppResult, ImportedSession, decode_bytes};

#[derive(Default)]
struct SecureCrtKeyFrame {
    name: String,
    fields: HashMap<String, String>,
}

pub(super) fn parse_securecrt(path: &Path) -> AppResult<Vec<ImportedSession>> {
    let raw =
        std::fs::read(path).map_err(|e| AppError::Config(format!("Cannot read file: {e}")))?;
    parse_securecrt_content(&decode_bytes(&raw))
}

fn parse_securecrt_content(content: &str) -> AppResult<Vec<ImportedSession>> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut stack: Vec<SecureCrtKeyFrame> = Vec::new();
    let mut current_field: Option<String> = None;
    let mut current_value = String::new();
    let mut sessions = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| AppError::Config(format!("Invalid SecureCRT XML: {e}")))?
        {
            Event::Start(e) if e.name().as_ref() == b"key" => {
                let name = xml_name_attr(&reader, &e)?.unwrap_or_default();
                stack.push(SecureCrtKeyFrame {
                    name,
                    fields: HashMap::new(),
                });
            }
            Event::Empty(e) if e.name().as_ref() == b"key" => {}
            Event::Start(e) if is_securecrt_value_tag(e.name().as_ref()) => {
                current_field = xml_name_attr(&reader, &e)?;
                current_value.clear();
            }
            Event::Empty(e) if is_securecrt_value_tag(e.name().as_ref()) => {
                if let (Some(frame), Some(name)) = (stack.last_mut(), xml_name_attr(&reader, &e)?) {
                    frame.fields.insert(name, String::new());
                }
            }
            Event::Text(e) => {
                if current_field.is_some() {
                    let decoded = reader.decoder().decode(e.as_ref()).map_err(|err| {
                        AppError::Config(format!("Invalid SecureCRT XML text: {err}"))
                    })?;
                    current_value.push_str(&decoded);
                }
            }
            Event::CData(e) => {
                if current_field.is_some() {
                    let decoded = reader.decoder().decode(e.as_ref()).map_err(|err| {
                        AppError::Config(format!("Invalid SecureCRT XML CDATA: {err}"))
                    })?;
                    current_value.push_str(&decoded);
                }
            }
            Event::End(e) if is_securecrt_value_tag(e.name().as_ref()) => {
                if let (Some(frame), Some(name)) = (stack.last_mut(), current_field.take()) {
                    frame.fields.insert(name, current_value.trim().to_string());
                }
                current_value.clear();
            }
            Event::End(e) if e.name().as_ref() == b"key" => {
                if let Some(frame) = stack.pop()
                    && let Some(session) = securecrt_session_from_frame(&frame, &stack)
                {
                    sessions.push(session);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(sessions)
}

fn is_securecrt_value_tag(name: &[u8]) -> bool {
    matches!(name, b"string" | b"dword")
}

fn xml_name_attr(reader: &Reader<&[u8]>, event: &BytesStart<'_>) -> AppResult<Option<String>> {
    for attr in event.attributes() {
        let attr = attr.map_err(|e| AppError::Config(format!("Invalid XML attribute: {e}")))?;
        if attr.key.as_ref() == b"name" {
            let value = attr
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                .map_err(|e| AppError::Config(format!("Invalid XML attribute value: {e}")))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn securecrt_session_from_frame(
    frame: &SecureCrtKeyFrame,
    ancestors: &[SecureCrtKeyFrame],
) -> Option<ImportedSession> {
    let sessions_index = ancestors
        .iter()
        .position(|frame| frame.name == "Sessions")?;
    let protocol = frame
        .fields
        .get("Protocol Name")
        .map(String::as_str)
        .unwrap_or("");
    if !protocol.eq_ignore_ascii_case("SSH2") {
        return None;
    }

    let host = frame.fields.get("Hostname")?.trim().to_string();
    if host.is_empty() {
        return None;
    }

    let port = frame
        .fields
        .get("[SSH2] Port")
        .or_else(|| frame.fields.get("Port"))
        .and_then(|port| port.parse::<u16>().ok())
        .filter(|port| *port > 0)
        .unwrap_or(22);

    let username = frame
        .fields
        .get("Username")
        .map(|username| username.trim())
        .filter(|username| !username.is_empty())
        .unwrap_or("root")
        .to_string();

    let group_path: Vec<String> = ancestors[sessions_index + 1..]
        .iter()
        .map(|frame| frame.name.trim())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .collect();

    Some(ImportedSession {
        name: if frame.name.trim().is_empty() {
            host.clone()
        } else {
            frame.name.clone()
        },
        host,
        port,
        username,
        auth_type: "password".to_string(),
        group_path: if group_path.is_empty() {
            None
        } else {
            Some(group_path)
        },
        description: None,
    })
}
