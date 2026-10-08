use crate::plugins::manifest::{ActionManifest, ParameterKind, ResultKind};
use crate::plugins::{ErrorCode, MAX_TEXT_BYTES, PluginError, PluginResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ParameterValue {
    String(String),
    Integer(i64),
    Boolean(bool),
}

impl ParameterValue {
    pub fn text(&self) -> String {
        match self {
            Self::String(v) => v.clone(),
            Self::Integer(v) => v.to_string(),
            Self::Boolean(v) => v.to_string(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ActionInput {
    pub parameters: BTreeMap<String, ParameterValue>,
    pub text: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ActionResult {
    Command { title: String, command: String },
    Text(String),
}

pub fn parse_parameter(kind: ParameterKind, text: &str) -> PluginResult<ParameterValue> {
    let invalid = || {
        PluginError::new(
            ErrorCode::InvalidManifest,
            "Parameter does not match its declared type",
        )
    };
    if text.len() > MAX_TEXT_BYTES {
        return Err(invalid());
    }
    match kind {
        ParameterKind::String => Ok(ParameterValue::String(text.into())),
        ParameterKind::Integer => text
            .parse()
            .map(ParameterValue::Integer)
            .map_err(|_| invalid()),
        ParameterKind::Boolean => text
            .parse()
            .map(ParameterValue::Boolean)
            .map_err(|_| invalid()),
    }
}

pub fn validate_input(action: &ActionManifest, input: &ActionInput) -> PluginResult<ActionInput> {
    let invalid = || {
        PluginError::new(
            ErrorCode::InvalidManifest,
            "Check action parameters and input size",
        )
    };
    let mut normalized = input.clone();
    if input
        .parameters
        .keys()
        .any(|key| !action.parameters.iter().any(|p| p.id == *key))
    {
        return Err(invalid());
    }
    for parameter in &action.parameters {
        if !normalized.parameters.contains_key(&parameter.id) {
            if let Some(value) = &parameter.default {
                normalized.parameters.insert(
                    parameter.id.clone(),
                    parse_parameter(parameter.kind, value)?,
                );
            } else if parameter.required {
                return Err(invalid());
            }
        }
        if let Some(value) = normalized.parameters.get(&parameter.id) {
            let compatible = matches!(
                (parameter.kind, value),
                (ParameterKind::String, ParameterValue::String(_))
                    | (ParameterKind::Integer, ParameterValue::Integer(_))
                    | (ParameterKind::Boolean, ParameterValue::Boolean(_))
            );
            if !compatible {
                return Err(invalid());
            }
        }
    }
    let bytes = normalized
        .parameters
        .iter()
        .map(|(k, v)| k.len() + v.text().len())
        .sum::<usize>()
        + input.text.as_ref().map_or(0, String::len);
    if bytes > MAX_TEXT_BYTES || (action.result == ResultKind::Command && input.text.is_some()) {
        return Err(invalid());
    }
    Ok(normalized)
}

pub fn validate_result(kind: ResultKind, result: &ActionResult) -> PluginResult<()> {
    let valid = match (kind, result) {
        (ResultKind::Command, ActionResult::Command { title, command }) => {
            title.len() <= 160
                && !command.is_empty()
                && command.len() <= MAX_TEXT_BYTES
                && ordinary_text(title)
                && ordinary_text(command)
        }
        (ResultKind::Text, ActionResult::Text(text)) => {
            text.len() <= MAX_TEXT_BYTES && ordinary_text(text)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(PluginError::new(
            ErrorCode::InvalidResult,
            "Plugin returned an invalid, oversized or control-bearing result",
        ))
    }
}

pub fn ordinary_text(text: &str) -> bool {
    text.chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}
