use zzclawterm_plugin_api::{
    ActionInput, ActionResult, CommandDraft, ErrorKind, Plugin, PluginError, Value,
};

#[derive(Default)]
struct Diagnostics {
    calls: u64,
}

impl Plugin for Diagnostics {
    fn invoke(&mut self, input: ActionInput) -> Result<ActionResult, PluginError> {
        if input.action != "diagnose" {
            return Err(PluginError {
                kind: ErrorKind::UnsupportedAction,
                message: "Choose Diagnose".into(),
            });
        }
        let host = input
            .arguments
            .iter()
            .find_map(|p| match (&*p.name, &p.value) {
                ("host", Value::Text(v)) => Some(v),
                _ => None,
            })
            .ok_or_else(|| PluginError {
                kind: ErrorKind::InvalidInput,
                message: "Enter a hostname".into(),
            })?;
        if host.is_empty()
            || host.len() > 253
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        {
            return Err(PluginError {
                kind: ErrorKind::InvalidInput,
                message: "Use a hostname containing letters, digits, dots and hyphens".into(),
            });
        }
        self.calls += 1;
        Ok(ActionResult::Command(CommandDraft {
            title: format!("Diagnostic draft {}", self.calls),
            command: format!("nslookup {host}\nping {host}"),
        }))
    }
}

zzclawterm_plugin_api::register_plugin!(Diagnostics);
