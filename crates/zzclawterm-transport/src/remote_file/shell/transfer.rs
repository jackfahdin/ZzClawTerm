use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{Stream, StreamExt as _};
use russh::{ChannelMsg, Disconnect};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::time::Instant;

use crate::download_path::{DownloadTargetAccess, DownloadTemporary};
use crate::{SftpTransferControl, SftpTransferOptions, open_authenticated_ssh_handle};

use super::{REMOTE_FILE_COMMAND_TIMEOUT, ShellRemote, shell_quote, shell_quote_path};

#[cfg(test)]
mod tests;

/// Uploads and replies share one inactivity clock: a server need not produce
/// stdout while it is still consuming a large stdin stream.
#[derive(Clone)]
struct TransferActivity {
    last: Arc<Mutex<Instant>>,
    timeout: Duration,
}

impl TransferActivity {
    fn new(timeout: Duration) -> Self {
        Self {
            last: Arc::new(Mutex::new(Instant::now())),
            timeout,
        }
    }

    fn touch(&self) {
        *self.last.lock().expect("transfer activity lock") = Instant::now();
    }

    async fn step<T>(
        &self,
        control: &SftpTransferControl,
        operation: impl Future<Output = T>,
    ) -> anyhow::Result<T> {
        tokio::pin!(operation);
        loop {
            control.check_cancelled()?;
            if control.is_paused() {
                self.touch();
            } else if self.last.lock().expect("transfer activity lock").elapsed() >= self.timeout {
                anyhow::bail!("remote file transfer timed out");
            }
            tokio::select! {
                result = &mut operation, if !control.is_paused() => {
                    self.touch();
                    return Ok(result);
                }
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
        }
    }
}

enum StreamFile {
    Upload(PathBuf),
    Download {
        root: PathBuf,
        target: PathBuf,
        expected_size: Option<u64>,
    },
}

impl ShellRemote {
    pub(in crate::remote_file) fn upload_stream<F>(
        &self,
        local: &Path,
        remote: &str,
        control: SftpTransferControl,
        options: &SftpTransferOptions,
        progress: F,
    ) -> anyhow::Result<u64>
    where
        F: FnMut(u64) + Send + 'static,
    {
        let length = std::fs::metadata(local)?.len();
        let temporary = format!(
            "{remote}.zzclawterm-upload-{}",
            uuid::Uuid::new_v4().simple()
        );
        let mode = options
            .default_file_mode
            .map(|mode| format!("chmod {mode:o} {} && ", shell_quote_path(&temporary)))
            .unwrap_or_default();
        let command = format!(
            "umask 077; cat > {temp} && test $(wc -c < {temp}) -eq {length} && {mode}mv -f -- {temp} {target}",
            temp = shell_quote(&temporary),
            target = shell_quote(remote),
        );
        let result = self.stream_file(
            command,
            StreamFile::Upload(local.to_path_buf()),
            control,
            options.buffer_size_bytes(),
            progress,
        );
        if result.is_err() {
            let _ = self.exec(format!("rm -f -- {}", shell_quote(&temporary)), None);
        }
        result
    }

    pub(in crate::remote_file) fn download_stream<F>(
        &self,
        remote: &str,
        local: &Path,
        root: &Path,
        expected_size: Option<u64>,
        control: SftpTransferControl,
        progress: F,
    ) -> anyhow::Result<u64>
    where
        F: FnMut(u64) + Send + 'static,
    {
        self.stream_file(
            format!("cat -- {}", shell_quote(remote)),
            StreamFile::Download {
                root: root.to_path_buf(),
                target: local.to_path_buf(),
                expected_size,
            },
            control,
            crate::sftp_transfer_types::SFTP_TRANSFER_DEFAULT_BUFFER_SIZE,
            progress,
        )
    }

    fn stream_file<F>(
        &self,
        command: String,
        file: StreamFile,
        control: SftpTransferControl,
        buffer_size: usize,
        mut progress: F,
    ) -> anyhow::Result<u64>
    where
        F: FnMut(u64) + Send + 'static,
    {
        let config = self.config.clone();
        let multiplex = self.multiplex.clone();
        let operation = async move {
            let activity = TransferActivity::new(REMOTE_FILE_COMMAND_TIMEOUT);
            let (handle, owned) = if let Some(multiplex) = multiplex {
                (
                    activity
                        .step(&control, multiplex.exec_target_handle())
                        .await?,
                    None,
                )
            } else {
                let (handle, jumps) = activity
                    .step(&control, open_authenticated_ssh_handle(&config))
                    .await??;
                // A dedicated handle is not shared with the interactive session.
                (Arc::new(tokio::sync::Mutex::new(handle)), Some(jumps))
            };
            let result = async {
                let channel = {
                    let handle = activity.step(&control, handle.lock()).await?;
                    activity
                        .step(&control, handle.channel_open_session())
                        .await??
                };
                let (reader, writer) = channel.split();
                let result = async {
                    activity
                        .step(&control, writer.exec(true, command.into_bytes()))
                        .await??;
                    let messages =
                        Box::pin(futures::stream::unfold(reader, |mut reader| async move {
                            reader.wait().await.map(|message| (message, reader))
                        }));
                    match file {
                        StreamFile::Upload(path) => {
                            let mut input = tokio::fs::File::open(path).await?;
                            let mut output = writer.make_writer();
                            let send = async {
                                let bytes = send_file(
                                    &mut input,
                                    &mut output,
                                    buffer_size,
                                    &control,
                                    &activity,
                                    &mut progress,
                                )
                                .await?;
                                activity.step(&control, writer.eof()).await??;
                                Ok::<_, anyhow::Error>(bytes)
                            };
                            let mut ignore_progress = |_| {};
                            let receive = receive_file(
                                messages,
                                tokio::io::sink(),
                                false,
                                &control,
                                &activity,
                                &mut ignore_progress,
                            );
                            let (bytes, _) = tokio::try_join!(send, receive)?;
                            Ok(bytes)
                        }
                        StreamFile::Download {
                            root,
                            target,
                            expected_size,
                        } => {
                            let access = DownloadTargetAccess::capture(&root, &target)?;
                            let (temporary, mut output) =
                                DownloadTemporary::prepare_with_access(&root, &target, 0, access)
                                    .await?;
                            let result = async {
                                activity.step(&control, writer.eof()).await??;
                                let bytes = receive_file(
                                    messages,
                                    &mut output,
                                    true,
                                    &control,
                                    &activity,
                                    &mut progress,
                                )
                                .await?;
                                activity.step(&control, output.sync_all()).await??;
                                Ok::<_, anyhow::Error>(bytes)
                            }
                            .await;
                            drop(output);
                            let bytes = result?;
                            anyhow::ensure!(
                                expected_size.is_none_or(|expected| expected == bytes),
                                "remote download size changed during transfer"
                            );
                            control.wait_if_paused().await?;
                            temporary.commit_async(&root, &target).await?;
                            Ok(bytes)
                        }
                    }
                }
                .await;
                let _ = tokio::time::timeout(Duration::from_secs(1), writer.close()).await;
                result
            }
            .await;
            if let Some(jumps) = owned {
                let handle = handle.lock().await;
                let _ = handle
                    .disconnect(Disconnect::ByApplication, "file transfer completed", "en")
                    .await;
                for jump in jumps {
                    let _ = jump
                        .disconnect(Disconnect::ByApplication, "file transfer completed", "en")
                        .await;
                }
            }
            result
        };
        if let Some(multiplex) = self.multiplex.as_ref() {
            multiplex.block_on(operation)
        } else {
            crate::run_ssh_exec_operation(operation)
        }
    }
}

async fn send_file<R, W, F>(
    input: &mut R,
    output: &mut W,
    buffer_size: usize,
    control: &SftpTransferControl,
    activity: &TransferActivity,
    progress: &mut F,
) -> anyhow::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut(u64),
{
    let mut buffer = vec![0_u8; buffer_size];
    let mut bytes = 0_u64;
    loop {
        let read = activity.step(control, input.read(&mut buffer)).await??;
        if read == 0 {
            break;
        }
        activity
            .step(control, output.write_all(&buffer[..read]))
            .await??;
        bytes = bytes.saturating_add(read as u64);
        progress(bytes);
    }
    activity.step(control, output.flush()).await??;
    Ok(bytes)
}

async fn receive_file<S, W, F>(
    mut messages: S,
    mut output: W,
    download: bool,
    control: &SftpTransferControl,
    activity: &TransferActivity,
    progress: &mut F,
) -> anyhow::Result<u64>
where
    S: Stream<Item = ChannelMsg> + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut(u64),
{
    let mut bytes = 0_u64;
    let mut exit_status = None;
    let mut eof = false;
    loop {
        match activity.step(control, messages.next()).await? {
            Some(ChannelMsg::Data { data }) if download => {
                activity.step(control, output.write_all(&data)).await??;
                bytes = bytes.saturating_add(data.len() as u64);
                progress(bytes);
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => {
                exit_status = Some(status);
                if eof {
                    break;
                }
            }
            Some(ChannelMsg::Eof) => {
                eof = true;
                if exit_status.is_some() {
                    break;
                }
            }
            Some(ChannelMsg::Close) | None => break,
            Some(ChannelMsg::Failure) => anyhow::bail!("remote file transfer request failed"),
            _ => {}
        }
    }
    anyhow::ensure!(
        exit_status == Some(0),
        "remote file transfer failed with status {}",
        exit_status.unwrap_or(255)
    );
    Ok(bytes)
}
