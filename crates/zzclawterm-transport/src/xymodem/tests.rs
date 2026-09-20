use std::io::Cursor;
use std::time::Duration;

use super::{
    ACK, CAN, CRC_REQUEST, EOT, NAK, SOH, STX, XymodemAction, XymodemError, XymodemFile,
    XymodemOptions, XymodemProtocol, XymodemSender, crc16,
};

fn sender(protocol: XymodemProtocol, sizes: &[usize]) -> XymodemSender {
    let files = sizes
        .iter()
        .enumerate()
        .map(|(index, size)| {
            XymodemFile::new(
                format!("file-{index}.bin"),
                *size as u64,
                Cursor::new(vec![index as u8 + 7; *size]),
            )
        })
        .collect();
    XymodemSender::new(protocol, files, XymodemOptions::default()).unwrap()
}

fn sent(actions: &[XymodemAction]) -> Vec<u8> {
    actions
        .iter()
        .find_map(|action| match action {
            XymodemAction::Send(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .expect("outgoing frame")
}

fn feed(sender: &mut XymodemSender, bytes: &[u8]) -> Vec<XymodemAction> {
    sender.feed(bytes, Duration::ZERO)
}

#[test]
fn checksum_and_crc_frames_validate_padding_and_ack_only_progress() {
    assert_eq!(crc16(b"123456789"), 0x31c3);
    for request in [NAK, CRC_REQUEST] {
        let mut sender = sender(XymodemProtocol::Xmodem, &[3]);
        let actions = feed(&mut sender, &[request]);
        let frame = sent(&actions);
        assert_eq!(&frame[..3], &[SOH, 1, 254]);
        assert_eq!(&frame[3..6], &[7; 3]);
        assert!(frame[6..131].iter().all(|byte| *byte == 0x1a));
        if request == CRC_REQUEST {
            assert_eq!(&frame[131..], &crc16(&frame[3..131]).to_be_bytes());
        } else {
            assert_eq!(
                frame[131],
                frame[3..131]
                    .iter()
                    .fold(0u8, |sum, b| sum.wrapping_add(*b))
            );
        }
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, XymodemAction::Progress(p) if p.bytes_transferred == 0))
        );
        assert_eq!(sent(&feed(&mut sender, &[NAK])), frame);
        let acked = feed(&mut sender, &[ACK]);
        assert_eq!(sent(&acked), vec![EOT]);
        assert!(acked.iter().any(|a| matches!(a, XymodemAction::Progress(p) if p.bytes_transferred == 3 && p.files_completed == 0)));
        assert!(
            feed(&mut sender, &[ACK])
                .iter()
                .any(|a| matches!(a, XymodemAction::Finished(Ok(()))))
        );
    }
}

#[test]
fn repeated_crc_requests_retry_first_block_and_sequence_wraps() {
    let mut sender = sender(XymodemProtocol::Xmodem, &[128 * 256]);
    let first = sent(&feed(&mut sender, &[NAK]));
    let upgraded = sent(&feed(&mut sender, &[CRC_REQUEST]));
    assert_eq!(&first[..131], &upgraded[..131]);
    assert_eq!(upgraded.len(), 133);
    assert_eq!(sent(&feed(&mut sender, &[CRC_REQUEST])), upgraded);
    for sequence in 2..=256 {
        let frame = sent(&feed(&mut sender, &[ACK]));
        assert_eq!(frame[1], sequence as u8);
        assert_eq!(frame[2], !(sequence as u8));
    }
    assert_eq!(sent(&feed(&mut sender, &[ACK])), vec![EOT]);
}

#[test]
fn memory_peer_accepts_ymodem_batch_with_empty_file_and_double_eot() {
    let mut sender = sender(XymodemProtocol::Ymodem, &[1025, 0, 3]);
    let mut accepted = Vec::new();
    for (index, size) in [1025, 0, 3].into_iter().enumerate() {
        let header = sent(&feed(&mut sender, &[CRC_REQUEST]));
        assert_eq!(&header[..3], &[SOH, 0, 255]);
        assert!(header[3..].starts_with(format!("file-{index}.bin\0{size}\0").as_bytes()));
        let mut next = feed(&mut sender, &[ACK, CRC_REQUEST]);
        let mut data = Vec::new();
        while sent(&next) != vec![EOT] {
            let frame = sent(&next);
            let block = if frame[0] == STX { 1024 } else { 128 };
            assert_eq!(
                &frame[block + 3..],
                &crc16(&frame[3..block + 3]).to_be_bytes()
            );
            data.extend_from_slice(&frame[3..block + 3]);
            next = feed(&mut sender, &[ACK]);
        }
        data.truncate(size);
        assert_eq!(data, vec![index as u8 + 7; size]);
        assert_eq!(sent(&feed(&mut sender, &[NAK])), vec![EOT]);
        accepted.extend(feed(&mut sender, &[ACK]));
    }
    let end = sent(&feed(&mut sender, &[CRC_REQUEST]));
    assert!(end[3..131].iter().all(|b| *b == 0));
    assert!(
        feed(&mut sender, &[ACK])
            .iter()
            .any(|a| matches!(a, XymodemAction::Finished(Ok(()))))
    );
    assert!(accepted.iter().any(|a| matches!(a, XymodemAction::Progress(p) if p.bytes_transferred == 1028 && p.files_completed == 3)));
}

#[test]
fn timeouts_retransmit_identical_frames_and_noise_does_not_extend_deadlines() {
    let mut sender = sender(XymodemProtocol::Xmodem, &[1]);
    let frame = sent(&feed(&mut sender, &[CRC_REQUEST]));
    for retry in 1..=10 {
        assert!(
            sender
                .feed(b"noise", Duration::from_secs(retry * 10 - 1))
                .is_empty()
        );
        assert_eq!(sent(&sender.tick(Duration::from_secs(retry * 10))), frame);
    }
    assert!(
        sender
            .tick(Duration::from_secs(110))
            .iter()
            .any(|a| matches!(a, XymodemAction::Finished(Err(XymodemError::TimedOut))))
    );
    assert!(sender.is_done());
    assert!(sender.tick(Duration::from_secs(120)).is_empty());
}

#[test]
fn receiver_cancel_spans_input_chunks_and_local_cancel_is_idempotent() {
    let mut sender = sender(XymodemProtocol::Xmodem, &[1]);
    feed(&mut sender, &[CAN]);
    assert!(!sender.is_done());
    assert!(feed(&mut sender, &[CAN]).iter().any(|a| matches!(
        a,
        XymodemAction::Finished(Err(XymodemError::RemoteCancelled))
    )));
    assert!(sender.cancel().is_empty());
    let mut sender = self::sender(XymodemProtocol::Ymodem, &[1]);
    assert_eq!(sent(&sender.cancel()), vec![CAN; 8]);
    assert!(sender.cancel().is_empty());
}

#[test]
fn premature_eof_and_invalid_inputs_fail_without_content_in_errors() {
    let file = XymodemFile::new("a", 20, Cursor::new(b"private".to_vec()));
    let mut sender = XymodemSender::new(
        XymodemProtocol::Xmodem,
        vec![file],
        XymodemOptions::default(),
    )
    .unwrap();
    assert!(
        feed(&mut sender, &[NAK])
            .iter()
            .any(|a| matches!(a, XymodemAction::Finished(Err(XymodemError::ReadFailed))))
    );
    let files = vec![
        XymodemFile::new("A", 0, Cursor::new(vec![])),
        XymodemFile::new("a", 0, Cursor::new(vec![])),
    ];
    assert!(matches!(
        XymodemSender::new(XymodemProtocol::Ymodem, files, XymodemOptions::default()),
        Err(XymodemError::InvalidFilename)
    ));
    let files = vec![
        XymodemFile::new("a", 0, Cursor::new(vec![])),
        XymodemFile::new("b", 0, Cursor::new(vec![])),
    ];
    assert!(matches!(
        XymodemSender::new(XymodemProtocol::Xmodem, files, XymodemOptions::default()),
        Err(XymodemError::SingleFileRequired)
    ));
}
