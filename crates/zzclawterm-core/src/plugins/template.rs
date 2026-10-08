use crate::plugins::invocation::{ActionInput, ActionResult, validate_input, validate_result};
use crate::plugins::manifest::{ActionManifest, Quoting};
use crate::plugins::{ErrorCode, MAX_TEXT_BYTES, PluginError, PluginResult};

pub fn expand(
    action: &ActionManifest,
    template: &str,
    input: &ActionInput,
) -> PluginResult<ActionResult> {
    let input = validate_input(action, input)?;
    let command = substitute(action, template, |name| {
        input
            .parameters
            .get(name)
            .map(|value| value.text())
            .unwrap_or_default()
    })?;
    let result = ActionResult::Command {
        title: action.name.clone(),
        command,
    };
    validate_result(action.result, &result)?;
    Ok(result)
}

pub fn validate_template(action: &ActionManifest, template: &str) -> PluginResult<()> {
    substitute(action, template, |_| String::new()).map(|_| ())
}

fn substitute(
    action: &ActionManifest,
    template: &str,
    value: impl Fn(&str) -> String,
) -> PluginResult<String> {
    let invalid = || {
        PluginError::new(
            ErrorCode::InvalidManifest,
            "Template must use declared ${parameter} substitutions only",
        )
    };
    if template.len() > MAX_TEXT_BYTES {
        return Err(invalid());
    }
    let mut rest = template;
    let mut result = String::new();
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let end = rest.find('}').ok_or_else(invalid)?;
        let name = &rest[..end];
        if !action.parameters.iter().any(|p| p.id == name) {
            return Err(invalid());
        }
        let text = value(name);
        let quoted = match action.quoting {
            Quoting::Literal => text,
            Quoting::Posix => format!("'{}'", text.replace('\'', "'\\''")),
            Quoting::Powershell => format!("'{}'", text.replace('\'', "''")),
        };
        if result.len() + quoted.len() > MAX_TEXT_BYTES {
            return Err(invalid());
        }
        result.push_str(&quoted);
        rest = &rest[end + 1..];
    }
    if result.len() + rest.len() > MAX_TEXT_BYTES {
        return Err(invalid());
    }
    result.push_str(rest);
    Ok(result)
}
