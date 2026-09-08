use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::Context;
use nyaterm_core::{
    AppSettingsSummary, ExistingFileBehavior as CoreExistingFileBehavior, Group,
    RecordingMode as CoreRecordingMode, RecordingRotationPolicy as CoreRecordingRotationPolicy,
};
use nyaterm_transport::{
    ExistingFileBehavior as TransportExistingFileBehavior, RecordingContext, RecordingMode,
    RecordingProfile, RecordingRotationPolicy as TransportRecordingRotationPolicy,
};
use time::OffsetDateTime;
use time::macros::format_description;

use crate::features::formatting::recording_file_path;
use crate::features::{NyaTermApp, runtime_jobs::await_blocking_job};
use crate::models::{RecordingPathPromptKind, RecordingPathPromptResult, SessionLaunchConfig};

impl NyaTermApp {
    pub(in crate::features) fn toggle_active_session_recording(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session.active_id_owned() else {
            self.shell
                .set_status("start a session before recording".to_string());
            cx.notify();
            return;
        };
        if self.recording.busy_action(&session_id).is_some() {
            return;
        }
        if self.recording.is_recording(&session_id) {
            self.stop_recording_for_session(&session_id, cx);
            return;
        }
        let session_name = self
            .session
            .ordered_sessions()
            .into_iter()
            .find(|session| session.id == session_id)
            .map(|session| session.name)
            .unwrap_or_else(|| session_id.clone());
        self.prompt_recording_path_for_session(
            RecordingPathPromptKind::Start,
            session_id,
            session_name,
            cx,
        );
    }

    pub(in crate::features) fn prompt_recording_path_for_session(
        &mut self,
        kind: RecordingPathPromptKind,
        session_id: String,
        session_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.remote_desktop.is_session(&session_id) {
            self.shell
                .set_status("recording is not supported for RDP sessions".to_string());
            cx.notify();
            return;
        }
        if !self.recording.begin_path_prompt(kind) {
            self.shell
                .set_status("recording path picker is already open".to_string());
            cx.notify();
            return;
        }
        let exists = self
            .session
            .metadata(&session_id)
            .is_some_and(|metadata| !metadata.disconnected);
        if !exists {
            self.recording.finish_path_prompt();
            self.shell
                .set_status("session no longer exists".to_string());
            self.remove_session_state(&session_id, cx);
            cx.notify();
            return;
        }
        let target = recording_file_path(
            self.settings.summary(),
            self.runtime.config_dir(),
            &session_name,
        );
        let directory = target
            .parent()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.runtime.config_dir().to_path_buf());
        let file_name = target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("nyaterm-recording.log");
        let receiver = cx.prompt_for_new_path(&directory, Some(file_name));
        self.shell.set_status(match kind {
            RecordingPathPromptKind::Start => "selecting recording path".to_string(),
            RecordingPathPromptKind::SaveTranscript => "selecting transcript path".to_string(),
        });
        cx.spawn(async move |this, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => RecordingPathPromptResult::Selected(path),
                Ok(Ok(None)) => RecordingPathPromptResult::Cancelled,
                Ok(Err(error)) => RecordingPathPromptResult::Failed(error.to_string()),
                Err(_) => RecordingPathPromptResult::Closed,
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_recording_path_prompt_result(kind, session_id, result, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_recording_path_prompt_result(
        &mut self,
        kind: RecordingPathPromptKind,
        session_id: String,
        result: RecordingPathPromptResult,
        cx: &mut Context<Self>,
    ) {
        self.recording.finish_path_prompt();
        match result {
            RecordingPathPromptResult::Selected(path) => match kind {
                RecordingPathPromptKind::Start => {
                    self.start_recording_to_path(&session_id, path.display().to_string(), cx);
                }
                RecordingPathPromptKind::SaveTranscript => {
                    self.save_transcript_to_path(&session_id, path.display().to_string(), cx);
                }
            },
            RecordingPathPromptResult::Cancelled => {
                self.shell.set_status(match kind {
                    RecordingPathPromptKind::Start => "recording start cancelled".to_string(),
                    RecordingPathPromptKind::SaveTranscript => {
                        "transcript save cancelled".to_string()
                    }
                });
            }
            RecordingPathPromptResult::Failed(error) => {
                self.shell
                    .set_status(format!("recording path picker failed: {error}"));
            }
            RecordingPathPromptResult::Closed => {
                self.shell
                    .set_status("recording path picker closed before returning".to_string());
            }
        }
    }

    fn start_recording_to_path(&mut self, session_id: &str, path: String, cx: &mut Context<Self>) {
        self.start_recording_with_profile(session_id, Some(PathBuf::from(path)), None, cx);
    }

    pub(in crate::features) fn start_recording_for_session(
        &mut self,
        session_id: &str,
        mode: RecordingMode,
        cx: &mut Context<Self>,
    ) {
        self.start_recording_with_profile(session_id, None, Some(mode), cx);
    }

    fn start_recording_with_profile(
        &mut self,
        session_id: &str,
        explicit_path: Option<PathBuf>,
        requested_mode: Option<RecordingMode>,
        cx: &mut Context<Self>,
    ) {
        if !self.recording.begin_action(session_id, "record") {
            self.shell
                .set_status("recording operation already in progress".to_string());
            cx.notify();
            return;
        }
        let Some((context, mut profile)) = self.recording_profile_for_session(session_id) else {
            self.recording.finish_action(session_id);
            self.shell
                .set_status("recording start failed: session no longer exists".to_string());
            cx.notify();
            return;
        };
        if let Some(mode) = requested_mode {
            profile.mode = mode;
        }
        self.shell.set_status("starting recording".to_string());
        let writer = self.recording.writer();
        let job_session_id = session_id.to_string();
        let memory_limit = self.settings.summary().recording_memory_limit_bytes as usize;
        let task = self.blocking_jobs.submit_task("recording-start", move |_| {
            writer.start(
                job_session_id,
                context,
                profile,
                explicit_path,
                memory_limit,
            )
        });
        let result_session_id = session_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                this.recording.finish_action(&result_session_id);
                match result {
                    Ok(path)
                        if this
                            .session
                            .metadata(&result_session_id)
                            .is_some_and(|metadata| !metadata.disconnected) =>
                    {
                        this.shell.set_status(format!("recording started: {path}"));
                        this.append_terminal_log(format!("\n# recording started: {path}\n"));
                    }
                    Ok(_) => {
                        this.recording.cleanup_writer_session(&result_session_id);
                        this.shell.set_status(
                            "recording start cancelled because session closed".to_string(),
                        );
                    }
                    Err(error) => {
                        this.shell
                            .set_status(format!("recording start failed: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn stop_recording_for_session(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.recording.begin_action(session_id, "record") {
            self.shell
                .set_status("recording operation already in progress".to_string());
            cx.notify();
            return;
        }
        self.shell.set_status("stopping recording".to_string());
        let writer = self.recording.writer();
        let job_session_id = session_id.to_string();
        let task = self
            .blocking_jobs
            .submit_task("recording-stop", move |_| writer.stop(job_session_id));
        let result_session_id = session_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                this.recording.finish_action(&result_session_id);
                match result {
                    Ok(path) => {
                        this.shell.set_status(format!("recording saved: {path}"));
                        this.append_terminal_log(format!("\n# recording saved: {path}\n"));
                    }
                    Err(error) => {
                        this.shell
                            .set_status(format!("recording stop failed: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn save_transcript_to_path(&mut self, session_id: &str, path: String, cx: &mut Context<Self>) {
        if !self.recording.begin_action(session_id, "save") {
            self.shell
                .set_status("recording operation already in progress".to_string());
            cx.notify();
            return;
        }
        self.shell.set_status("saving transcript".to_string());
        let writer = self.recording.writer();
        let job_session_id = session_id.to_string();
        let memory_limit = self.settings.summary().recording_memory_limit_bytes as usize;
        let include_io_labels = self.settings.summary().recording_include_io_labels;
        let include_timestamps = self.settings.summary().recording_include_timestamps;
        let task = self
            .blocking_jobs
            .submit_task("recording-transcript", move |_| {
                writer.save_transcript(
                    job_session_id,
                    path,
                    include_io_labels,
                    include_timestamps,
                    memory_limit,
                )
            });
        let result_session_id = session_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                this.recording.finish_action(&result_session_id);
                match result {
                    Ok(path) => {
                        this.shell.set_status(format!("transcript saved: {path}"));
                        this.append_terminal_log(format!("\n# transcript saved: {path}\n"));
                    }
                    Err(error) => {
                        this.shell
                            .set_status(format!("transcript save failed: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn save_session_transcript_for_session(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.session_info(session_id) else {
            self.shell
                .set_status("transcript save failed: session no longer exists".to_string());
            cx.notify();
            return;
        };
        let path = match session_transcript_file_path(
            self.settings.summary(),
            &session.name,
            OffsetDateTime::now_utc(),
        ) {
            Ok(path) => path,
            Err(error) => {
                self.shell
                    .set_status(format!("transcript save failed: {error}"));
                cx.notify();
                return;
            }
        };
        self.save_transcript_to_path(session_id, path.display().to_string(), cx);
    }

    pub(in crate::features) fn maybe_auto_start_recording(
        &mut self,
        session_id: &str,
        session_name: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.effective_recording_auto_start(session_id) {
            return;
        }
        let _ = session_name;
        self.start_recording_with_profile(session_id, None, None, cx);
    }

    pub(in crate::features) fn cleanup_recording_for_session(&mut self, session_id: &str) {
        self.recording.cleanup_session(session_id);
    }

    pub(in crate::features) fn apply_recording_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        self.recording.set_search_draft(text);
        cx.notify();
    }
}

impl NyaTermApp {
    fn recording_profile_for_session(
        &self,
        session_id: &str,
    ) -> Option<(RecordingContext, RecordingProfile)> {
        let metadata = self.session.metadata(session_id)?;
        if metadata.disconnected {
            return None;
        }
        let summary = self.settings.summary();
        let (launch_name, protocol, host, port, username) =
            recording_launch_context(&metadata.launch_config);
        let session_name = self
            .session
            .session_info(session_id)
            .map(|session| session.name)
            .unwrap_or(launch_name);
        let saved_connection = metadata
            .source_connection_id
            .as_deref()
            .and_then(|connection_id| {
                self.connection_state
                    .connections()
                    .iter()
                    .find(|connection| connection.id == connection_id)
            });
        let context = RecordingContext {
            session_id: session_id.to_string(),
            session_name,
            connection_id: metadata.source_connection_id.clone(),
            connection_name: saved_connection.map(|connection| connection.name.clone()),
            group_path: saved_connection.and_then(|connection| {
                recording_group_path(
                    self.connection_state.groups(),
                    connection.group_id.as_deref(),
                )
            }),
            protocol,
            host,
            port,
            username,
            started_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        };
        let connection_recording =
            saved_connection.and_then(|connection| connection.recording.as_ref());
        let profile = RecordingProfile {
            mode: connection_recording
                .and_then(|settings| settings.mode)
                .map(map_recording_mode)
                .unwrap_or_else(|| map_recording_mode(summary.recording_default_mode)),
            base_path: recording_base_path(summary, self.runtime.config_dir()),
            path_template: connection_recording
                .and_then(|settings| settings.path_template.as_ref())
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| summary.recording_path_template.clone()),
            include_timestamps: connection_recording
                .and_then(|settings| settings.include_timestamps)
                .unwrap_or(summary.recording_include_timestamps),
            include_io_labels: summary.recording_include_io_labels,
            include_session_metadata: summary.recording_include_session_metadata,
            rotation: connection_recording
                .and_then(|settings| settings.rotation.as_ref())
                .map(map_recording_rotation)
                .unwrap_or_else(|| map_recording_rotation(&summary.recording_rotation)),
            existing_file_behavior: map_existing_file_behavior(
                summary.recording_existing_file_behavior,
            ),
            include_binary_transfer_payloads: summary.recording_include_binary_transfer_payloads,
        };
        Some((context, profile))
    }

    fn effective_recording_auto_start(&self, session_id: &str) -> bool {
        let Some(metadata) = self.session.metadata(session_id) else {
            return false;
        };
        metadata
            .source_connection_id
            .as_deref()
            .and_then(|connection_id| {
                self.connection_state
                    .connections()
                    .iter()
                    .find(|connection| connection.id == connection_id)
            })
            .and_then(|connection| connection.recording.as_ref())
            .and_then(|settings| settings.auto_start)
            .unwrap_or(self.settings.summary().recording_auto_start)
    }
}

fn recording_base_path(settings: &AppSettingsSummary, config_dir: &std::path::Path) -> PathBuf {
    if settings.recording_path.trim().is_empty() {
        default_recording_directory().unwrap_or_else(|| config_dir.join("recordings"))
    } else {
        PathBuf::from(settings.recording_path.trim())
    }
}

fn default_recording_directory() -> Option<PathBuf> {
    dirs::download_dir().or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
}

fn session_transcript_file_path(
    settings: &AppSettingsSummary,
    session_name: &str,
    now: OffsetDateTime,
) -> Result<PathBuf, String> {
    let directory = if settings.recording_path.trim().is_empty() {
        default_recording_directory()
            .ok_or_else(|| "failed to resolve Downloads directory".to_string())?
    } else {
        PathBuf::from(settings.recording_path.trim())
    };
    let timestamp = now
        .format(format_description!(
            "[year]-[month]-[day]T[hour]-[minute]-[second]"
        ))
        .map_err(|error| format!("failed to format transcript timestamp: {error}"))?;
    let file_name = format!(
        "session-{}-{timestamp}.log",
        nyaterm_transport::safe_recording_name(session_name)
    );
    Ok(first_available_path(&directory.join(file_name)))
}

fn first_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let file_name = extension.map_or_else(
            || format!("{stem}-{index}"),
            |extension| format!("{stem}-{index}.{extension}"),
        );
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search must find an available path")
}

fn recording_group_path(groups: &[Group], group_id: Option<&str>) -> Option<String> {
    let mut current = group_id?;
    let mut names = Vec::new();
    let mut visited = HashSet::new();
    for _ in 0..32 {
        if !visited.insert(current.to_string()) {
            break;
        }
        let group = groups.iter().find(|group| group.id == current)?;
        names.push(group.name.clone());
        let Some(parent) = group.parent_id.as_deref() else {
            break;
        };
        current = parent;
    }
    names.reverse();
    (!names.is_empty()).then(|| names.join("/"))
}

fn recording_launch_context(
    launch_config: &SessionLaunchConfig,
) -> (String, String, Option<String>, Option<u16>, Option<String>) {
    match launch_config {
        SessionLaunchConfig::Local(config) => {
            (config.name.clone(), "local".to_string(), None, None, None)
        }
        SessionLaunchConfig::Ssh(config) => (
            config.name.clone(),
            "ssh".to_string(),
            Some(config.host.clone()),
            Some(config.port),
            Some(config.username.clone()),
        ),
        SessionLaunchConfig::Telnet(config) => (
            config.name.clone(),
            "telnet".to_string(),
            Some(config.host.clone()),
            Some(config.port),
            Some(config.username.clone()),
        ),
        SessionLaunchConfig::Serial(config) => (
            config.name.clone(),
            "serial".to_string(),
            Some(config.port_name.clone()),
            None,
            None,
        ),
        SessionLaunchConfig::Rdp(config) => (
            config.name.clone(),
            "rdp".to_string(),
            Some(config.host.clone()),
            Some(config.port),
            Some(config.username.clone()),
        ),
        SessionLaunchConfig::Vnc(config) => (
            config.name.clone(),
            "vnc".to_string(),
            Some(config.host.clone()),
            Some(config.port),
            None,
        ),
    }
}

fn map_recording_mode(mode: CoreRecordingMode) -> RecordingMode {
    match mode {
        CoreRecordingMode::Raw => RecordingMode::Raw,
        CoreRecordingMode::Transcript => RecordingMode::Transcript,
    }
}

fn map_existing_file_behavior(behavior: CoreExistingFileBehavior) -> TransportExistingFileBehavior {
    match behavior {
        CoreExistingFileBehavior::Append => TransportExistingFileBehavior::Append,
        CoreExistingFileBehavior::Overwrite => TransportExistingFileBehavior::Overwrite,
        CoreExistingFileBehavior::Unique => TransportExistingFileBehavior::Unique,
    }
}

fn map_recording_rotation(
    rotation: &CoreRecordingRotationPolicy,
) -> TransportRecordingRotationPolicy {
    match rotation {
        CoreRecordingRotationPolicy::Daily => TransportRecordingRotationPolicy::Daily,
        CoreRecordingRotationPolicy::Size { max_bytes } => TransportRecordingRotationPolicy::Size {
            max_bytes: *max_bytes,
        },
        CoreRecordingRotationPolicy::Session => TransportRecordingRotationPolicy::Session,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use nyaterm_core::{AppSettingsSummary, Group};
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    use super::{first_available_path, recording_group_path, session_transcript_file_path};

    fn group(id: &str, name: &str, parent_id: Option<&str>) -> Group {
        Group {
            id: id.to_string(),
            name: name.to_string(),
            parent_id: parent_id.map(str::to_string),
            ..Group::default()
        }
    }

    #[test]
    fn session_transcript_path_uses_safe_name_and_utc_iso_timestamp() {
        let settings = AppSettingsSummary {
            recording_path: "/tmp/nyaterm-recordings".to_string(),
            ..AppSettingsSummary::default()
        };
        let now = PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::August, 15).expect("valid date"),
            Time::from_hms(7, 8, 9).expect("valid time"),
        )
        .assume_offset(UtcOffset::UTC);

        let path =
            session_transcript_file_path(&settings, "prod / shell", now).expect("transcript path");

        // Compare against a joined `PathBuf` rather than a `/`-separated literal so
        // the expectation matches the platform separator `join` emits.
        let expected = PathBuf::from("/tmp/nyaterm-recordings")
            .join("session-prod_shell-2026-08-15T07-08-09.log");
        assert_eq!(path, expected);
    }

    #[test]
    fn first_available_path_never_overwrites_an_existing_log() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nyaterm-recording-path-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join("session-demo.log");
        fs::write(&path, b"existing").expect("write existing log");
        fs::write(directory.join("session-demo-1.log"), b"existing")
            .expect("write first collision");

        assert_eq!(
            first_available_path(&path),
            directory.join("session-demo-2.log")
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn recording_group_path_is_nested_cycle_safe_and_bounded() {
        let nested = vec![
            group("root", "Production", None),
            group("region", "Shanghai", Some("root")),
            group("host", "Databases", Some("region")),
        ];
        assert_eq!(
            recording_group_path(&nested, Some("host")).as_deref(),
            Some("Production/Shanghai/Databases")
        );

        let cycle = vec![group("a", "A", Some("b")), group("b", "B", Some("a"))];
        assert_eq!(
            recording_group_path(&cycle, Some("a")).as_deref(),
            Some("B/A")
        );

        let broken = vec![group("child", "Child", Some("missing"))];
        assert_eq!(recording_group_path(&broken, Some("child")), None);

        let deep = (0..40)
            .map(|index| {
                group(
                    &format!("g{index}"),
                    &format!("G{index}"),
                    (index > 0).then(|| format!("g{}", index - 1)).as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            recording_group_path(&deep, Some("g39"))
                .expect("bounded group path")
                .split('/')
                .count(),
            32
        );
    }
}
