//! The file capability used before an SSH file session becomes ready.
use std::future::Future;
use std::pin::Pin;
use zzclawterm_transport::ssh_services::SshFileSessionProbe;
use zzclawterm_transport::{SshMultiplexHandle, SshSessionConfig};

pub struct SftpSessionProbe;

impl SshFileSessionProbe for SftpSessionProbe {
    fn probe(
        &self,
        config: SshSessionConfig,
        multiplex: SshMultiplexHandle,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async move { crate::sftp::probe_sftp_session(&config, &multiplex).await })
    }
}
