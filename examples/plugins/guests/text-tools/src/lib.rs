use zzclawterm_plugin_api::{ActionInput, ActionResult, ErrorKind, Plugin, PluginError};

#[derive(Default)]
struct TextTools;

impl Plugin for TextTools {
    fn invoke(&mut self, input: ActionInput) -> Result<ActionResult, PluginError> {
        let text = input.text.ok_or_else(|| PluginError {
            kind: ErrorKind::InvalidInput,
            message: "Provide text explicitly".into(),
        })?;
        let result = match input.action.as_str() {
            "sort" => {
                let mut lines: Vec<_> = text.lines().collect();
                lines.sort();
                lines.join("\n")
            }
            "json" => {
                let value: serde_json::Value =
                    serde_json::from_str(&text).map_err(|_| PluginError {
                        kind: ErrorKind::InvalidInput,
                        message: "Input is not valid JSON".into(),
                    })?;
                serde_json::to_string_pretty(&value).map_err(|_| PluginError {
                    kind: ErrorKind::Failed,
                    message: "Cannot format JSON".into(),
                })?
            }
            _ => {
                return Err(PluginError {
                    kind: ErrorKind::UnsupportedAction,
                    message: "Choose Sort or Format JSON".into(),
                });
            }
        };
        Ok(ActionResult::Text(result))
    }
}

zzclawterm_plugin_api::register_plugin!(TextTools);
