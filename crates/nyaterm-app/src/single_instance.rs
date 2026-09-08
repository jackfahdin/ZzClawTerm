use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow, bail};
use nyaterm_core::{
    ACTIVATION_QUEUE_CAPACITY, ActivationAck, ActivationAckStatus, ActivationQueueError,
    ActivationReceiver, ActivationRequest, ActivationSender, MAX_ACTIVATION_FRAME_BYTES,
    activation_channel, decode_activation_ack, decode_activation_request, encode_activation_ack,
    encode_activation_request,
};

const INSTANCE_DIR_NAME: &str = ".instance";
const OWNER_LOCK_NAME: &str = "owner.lock";
const ENDPOINT_NAME: &str = "activation.endpoint";
const CONNECT_DEADLINE: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_millis(40);

pub(crate) enum SingleInstanceOutcome {
    Owner(SingleInstanceOwner),
    Forwarded,
}

pub(crate) struct SingleInstanceOwner {
    lock_file: File,
    endpoint_path: PathBuf,
    stop: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
    activation_tx: ActivationSender,
    activation_rx: Option<ActivationReceiver>,
}

impl SingleInstanceOwner {
    pub(crate) fn activation_sender(&self) -> ActivationSender {
        self.activation_tx.clone()
    }

    pub(crate) fn take_activation_receiver(&mut self) -> ActivationReceiver {
        self.activation_rx
            .take()
            .expect("activation receiver can only be taken once")
    }
}

impl Drop for SingleInstanceOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(listener_thread) = self.listener_thread.take() {
            let _ = listener_thread.join();
        }
        let _ = std::fs::remove_file(&self.endpoint_path);
        let _ = self.lock_file.unlock();
    }
}

pub(crate) fn acquire(
    config_dir: &Path,
    initial_request: ActivationRequest,
) -> anyhow::Result<SingleInstanceOutcome> {
    let instance_dir = config_dir.join(INSTANCE_DIR_NAME);
    std::fs::create_dir_all(&instance_dir).with_context(|| {
        format!(
            "create single-instance directory '{}'",
            instance_dir.display()
        )
    })?;
    restrict_instance_directory(&instance_dir)?;
    let lock_path = instance_dir.join(OWNER_LOCK_NAME);
    let endpoint_path = instance_dir.join(ENDPOINT_NAME);
    let started = Instant::now();

    loop {
        let lock_file = open_private_file(&lock_path)?;
        match lock_file.try_lock() {
            Ok(()) => {
                return start_owner(lock_file, endpoint_path, initial_request)
                    .map(SingleInstanceOutcome::Owner);
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(error).context("acquire NyaTerm single-instance lock");
            }
        }

        if let Some(endpoint) = read_endpoint(&endpoint_path)?
            && forward_activation(endpoint, &initial_request).is_ok()
        {
            return Ok(SingleInstanceOutcome::Forwarded);
        }
        if started.elapsed() >= CONNECT_DEADLINE {
            bail!(
                "another NyaTerm instance owns this data directory but did not accept activation"
            );
        }
        thread::sleep(RETRY_DELAY);
    }
}

fn start_owner(
    lock_file: File,
    endpoint_path: PathBuf,
    initial_request: ActivationRequest,
) -> anyhow::Result<SingleInstanceOwner> {
    let _ = std::fs::remove_file(&endpoint_path);
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .context("bind NyaTerm activation listener")?;
    listener
        .set_nonblocking(true)
        .context("configure NyaTerm activation listener")?;
    let address = listener.local_addr()?;
    let auth_token = *uuid::Uuid::new_v4().as_bytes();
    write_endpoint(&endpoint_path, address, auth_token)?;

    let (activation_tx, activation_rx) = activation_channel(ACTIVATION_QUEUE_CAPACITY);
    activation_tx
        .try_send(initial_request)
        .map_err(|_| anyhow!("failed to enqueue initial activation"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let listener_stop = Arc::clone(&stop);
    let listener_activation_tx = activation_tx.clone();
    let listener_thread = thread::Builder::new()
        .name("nyaterm-activation-listener".to_string())
        .spawn(move || run_listener(listener, auth_token, listener_activation_tx, listener_stop))
        .context("spawn NyaTerm activation listener")?;

    Ok(SingleInstanceOwner {
        lock_file,
        endpoint_path,
        stop,
        listener_thread: Some(listener_thread),
        activation_tx,
        activation_rx: Some(activation_rx),
    })
}

fn run_listener(
    listener: TcpListener,
    auth_token: [u8; 16],
    activation_tx: ActivationSender,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let sender = activation_tx.clone();
                thread::spawn(move || handle_connection(stream, auth_token, sender));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    activation_tx.close();
}

fn handle_connection(mut stream: TcpStream, auth_token: [u8; 16], activation_tx: ActivationSender) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let mut frame = Vec::new();
    let read_result = (&mut stream)
        .take((MAX_ACTIVATION_FRAME_BYTES + 1) as u64)
        .read_to_end(&mut frame);
    let (request_id, status) = match read_result
        .ok()
        .filter(|_| frame.len() <= MAX_ACTIVATION_FRAME_BYTES)
        .and_then(|_| decode_activation_request(&frame).ok())
    {
        Some((received_token, request)) if received_token == auth_token => {
            let request_id = request.request_id;
            let status = match activation_tx.try_send(request) {
                Ok(()) => ActivationAckStatus::Accepted,
                Err(ActivationQueueError::Full) => ActivationAckStatus::QueueFull,
                Err(ActivationQueueError::Closed) => ActivationAckStatus::ShuttingDown,
            };
            (request_id, status)
        }
        Some((_, request)) => (request.request_id, ActivationAckStatus::InvalidRequest),
        None => ([0; 16], ActivationAckStatus::InvalidRequest),
    };
    if let Ok(ack) = encode_activation_ack(ActivationAck { request_id, status }) {
        let _ = stream.write_all(&ack);
        let _ = stream.flush();
    }
}

#[derive(Clone, Copy)]
struct ActivationEndpoint {
    address: SocketAddr,
    auth_token: [u8; 16],
}

fn forward_activation(
    endpoint: ActivationEndpoint,
    request: &ActivationRequest,
) -> anyhow::Result<()> {
    let frame = encode_activation_request(endpoint.auth_token, request)?;
    let mut stream = TcpStream::connect_timeout(&endpoint.address, IO_TIMEOUT)
        .context("connect to primary NyaTerm instance")?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(&frame)?;
    stream.shutdown(Shutdown::Write)?;
    let mut ack = Vec::new();
    (&mut stream)
        .take((MAX_ACTIVATION_FRAME_BYTES + 1) as u64)
        .read_to_end(&mut ack)?;
    let ack = decode_activation_ack(&ack)?;
    if ack.request_id != request.request_id {
        bail!("primary NyaTerm instance returned an unrelated activation ACK");
    }
    match ack.status {
        ActivationAckStatus::Accepted => Ok(()),
        ActivationAckStatus::QueueFull => bail!("primary NyaTerm activation queue is full"),
        ActivationAckStatus::ShuttingDown => bail!("primary NyaTerm instance is shutting down"),
        ActivationAckStatus::InvalidRequest => {
            bail!("primary NyaTerm instance rejected the activation request")
        }
    }
}

fn write_endpoint(
    endpoint_path: &Path,
    address: SocketAddr,
    auth_token: [u8; 16],
) -> anyhow::Result<()> {
    let temporary_path = endpoint_path.with_extension(format!("tmp-{}", std::process::id()));
    let token = auth_token
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let content = format!("NTSI1\n{}\n{token}\n", address.port());
    let mut file = open_private_file(&temporary_path)?;
    file.set_len(0)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary_path, endpoint_path).with_context(|| {
        format!(
            "publish NyaTerm activation endpoint '{}'",
            endpoint_path.display()
        )
    })
}

fn read_endpoint(endpoint_path: &Path) -> anyhow::Result<Option<ActivationEndpoint>> {
    let content = match std::fs::read_to_string(endpoint_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read NyaTerm activation endpoint"),
    };
    let mut lines = content.lines();
    if lines.next() != Some("NTSI1") {
        return Ok(None);
    }
    let Some(port) = lines.next().and_then(|value| value.parse::<u16>().ok()) else {
        return Ok(None);
    };
    let Some(token) = lines.next().and_then(parse_token) else {
        return Ok(None);
    };
    Ok(Some(ActivationEndpoint {
        address: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
        auth_token: token,
    }))
}

fn parse_token(value: &str) -> Option<[u8; 16]> {
    if value.len() != 32 {
        return None;
    }
    let mut token = [0; 16];
    for (index, byte) in token.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(token)
}

fn open_private_file(path: &Path) -> anyhow::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("open private instance file '{}'", path.display()))
}

#[cfg_attr(not(unix), allow(unused_variables))]
fn restrict_instance_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).with_context(
            || {
                format!(
                    "restrict single-instance directory permissions '{}'",
                    path.display()
                )
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::{Shutdown, TcpStream};
    use std::time::{SystemTime, UNIX_EPOCH};

    use nyaterm_core::{
        ActivationAck, ActivationAckStatus, ActivationRequest, RawActivationArg,
        decode_activation_ack, encode_activation_request,
    };

    use super::{
        ActivationEndpoint, SingleInstanceOutcome, acquire, forward_activation, read_endpoint,
    };

    fn request(id: u8) -> ActivationRequest {
        ActivationRequest {
            request_id: [id; 16],
            args: vec![RawActivationArg::Bytes(vec![id])],
        }
    }

    fn temporary_root(test_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nyaterm-single-instance-{test_name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn send_frame(endpoint: ActivationEndpoint, frame: &[u8]) -> ActivationAck {
        let mut stream = TcpStream::connect(endpoint.address).unwrap();
        stream.write_all(frame).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut ack = Vec::new();
        stream.read_to_end(&mut ack).unwrap();
        decode_activation_ack(&ack).unwrap()
    }

    #[test]
    fn secondary_forwards_to_owner_and_lock_recovers_after_drop() {
        let root = temporary_root("handoff");

        let SingleInstanceOutcome::Owner(mut owner) = acquire(&root, request(1)).unwrap() else {
            panic!("first process must own the instance");
        };
        let mut receiver = owner.take_activation_receiver();
        assert_eq!(
            futures::executor::block_on(receiver.recv()),
            Some(request(1))
        );
        assert!(matches!(
            acquire(&root, request(2)).unwrap(),
            SingleInstanceOutcome::Forwarded
        ));
        assert_eq!(
            futures::executor::block_on(receiver.recv()),
            Some(request(2))
        );
        drop(receiver);
        drop(owner);

        let third = acquire(&root, request(3)).unwrap();
        assert!(matches!(third, SingleInstanceOutcome::Owner(_)));
        drop(third);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_auth_and_malformed_frames_are_rejected_without_enqueueing() {
        let root = temporary_root("rejection");
        let SingleInstanceOutcome::Owner(mut owner) = acquire(&root, request(1)).unwrap() else {
            panic!("first process must own the instance");
        };
        let mut receiver = owner.take_activation_receiver();
        assert_eq!(
            futures::executor::block_on(receiver.recv()),
            Some(request(1))
        );
        let endpoint = read_endpoint(&owner.endpoint_path).unwrap().unwrap();

        let mut wrong_token = endpoint.auth_token;
        wrong_token[0] ^= 0xff;
        let rejected_request = request(2);
        let rejected_ack = send_frame(
            endpoint,
            &encode_activation_request(wrong_token, &rejected_request).unwrap(),
        );
        assert_eq!(rejected_ack.request_id, rejected_request.request_id);
        assert_eq!(rejected_ack.status, ActivationAckStatus::InvalidRequest);

        let malformed_ack = send_frame(endpoint, b"not-an-activation-frame");
        assert_eq!(malformed_ack.request_id, [0; 16]);
        assert_eq!(malformed_ack.status, ActivationAckStatus::InvalidRequest);

        forward_activation(endpoint, &request(3)).unwrap();
        assert_eq!(
            futures::executor::block_on(receiver.recv()),
            Some(request(3))
        );

        drop(receiver);
        drop(owner);
        let _ = std::fs::remove_dir_all(root);
    }
}
