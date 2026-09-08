use std::sync::Mutex;

use nyaterm_core::WebdavSyncSettings;
use zed_reqwest::StatusCode;

use super::aliyun::{
    AliyunDriveType, aliyun_drive_error_code, aliyun_drive_remote_error,
    parse_aliyun_drive_list_page,
};
use super::google_drive::{google_drive_multipart_body, parse_google_drive_list_page};
use super::helpers::{
    build_digest_authorization, form_urlencoded, parse_digest_challenge, percent_encode_path,
};
use super::onedrive::{onedrive_item_path, parse_onedrive_list_page};
use super::s3::parse_s3_list_page;
use super::webdav::parse_webdav_file_names;
use super::{NativeAliyunDriveRemote, NativeOneDriveRemote, NativeWebdavRemote};

#[test]
fn webdav_url_joins_endpoint_root_and_sync_path() {
    let remote = NativeWebdavRemote::new(&WebdavSyncSettings {
        endpoint: "https://dav.example.com/remote.php/webdav/".to_string(),
        root: "/apps/nyaterm/".to_string(),
        username: String::new(),
        password: None,
    })
    .expect("remote");

    assert_eq!(
        remote.url_for("/nyaterm/sync/latest.redb"),
        "https://dav.example.com/remote.php/webdav/apps/nyaterm/nyaterm/sync/latest.redb"
    );
}

#[test]
fn webdav_digest_authorization_matches_rfc_example() {
    let header = build_digest_authorization(
        r#"realm="testrealm@host.com", qop="auth", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
        "Mufasa",
        "Circle Of Life",
        "GET",
        "/dir/index.html",
        "0a4f113b",
        "00000001",
    )
    .expect("digest header");

    assert!(header.contains("Digest username=\"Mufasa\""));
    assert!(header.contains("qop=auth"));
    assert!(header.contains("response=\"6629fae49393a05397450978507c4ef1\""));
}

#[test]
fn webdav_digest_parser_handles_quoted_commas() {
    let parsed = parse_digest_challenge(
        r#"realm="Nya,Term", nonce="abc", algorithm=MD5, qop="auth,auth-int""#,
    );

    assert_eq!(parsed.get("realm").map(String::as_str), Some("Nya,Term"));
    assert_eq!(parsed.get("nonce").map(String::as_str), Some("abc"));
    assert_eq!(parsed.get("qop").map(String::as_str), Some("auth,auth-int"));
}

#[test]
fn webdav_propfind_parser_uses_local_names_and_decodes_file_names() {
    let body = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response><d:href>/dav/nyaterm/sync/snapshots/</d:href></d:response>
          <d:response><d:href>/dav/nyaterm/sync/snapshots/rev%20one.redb.enc</d:href></d:response>
          <d:response><d:href>/dav/nyaterm/sync/snapshots/ignored.txt</d:href></d:response>
          <response xmlns="DAV:"><href>/dav/nyaterm/sync/snapshots/rev-two.redb.enc</href></response>
        </d:multistatus>"#;

    assert_eq!(
        parse_webdav_file_names(body).expect("parse PROPFIND"),
        vec!["rev one.redb.enc", "rev-two.redb.enc"]
    );
}

#[test]
fn webdav_propfind_parser_rejects_invalid_percent_encoding() {
    let body = r#"<multistatus xmlns="DAV:"><response><href>/snapshots/bad%ZZ.redb.enc</href></response></multistatus>"#;

    assert!(parse_webdav_file_names(body).is_err());
}

#[test]
fn s3_list_parser_reads_keys_and_continuation_token() {
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
        <ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
          <IsTruncated>true</IsTruncated>
          <Contents><Key>root/sync/snapshots/r1.redb.enc</Key></Contents>
          <Contents><Key>root/sync/snapshots/r2.redb.enc</Key></Contents>
          <NextContinuationToken>token+/=</NextContinuationToken>
        </ListBucketResult>"#;
    let page = parse_s3_list_page(body).expect("parse S3 list");

    assert_eq!(
        page.keys,
        vec![
            "root/sync/snapshots/r1.redb.enc",
            "root/sync/snapshots/r2.redb.enc"
        ]
    );
    assert!(page.truncated);
    assert_eq!(page.next_token.as_deref(), Some("token+/="));
}

#[test]
fn google_drive_multipart_body_contains_metadata_and_media() {
    let (boundary, body) =
        google_drive_multipart_body("root", "latest.redb", b"payload").expect("multipart");
    let text = String::from_utf8(body).expect("utf8 multipart");

    assert!(text.contains(&format!("--{boundary}\r\n")));
    assert!(text.contains(r#""name":"latest.redb""#));
    assert!(text.contains(r#""parents":["root"]"#));
    assert!(text.contains("Content-Type: application/octet-stream\r\n\r\npayload"));
    assert!(text.ends_with(&format!("--{boundary}--\r\n")));
}

#[test]
fn drive_list_page_parsers_keep_files_and_pagination_tokens() {
    let google = parse_google_drive_list_page(
        r#"{"files":[{"name":"r1.redb.enc","mimeType":"application/octet-stream"},{"name":"folder","mimeType":"application/vnd.google-apps.folder"}],"nextPageToken":"google-next"}"#,
    )
    .expect("Google Drive page");
    assert_eq!(google.names, vec!["r1.redb.enc"]);
    assert_eq!(google.next_page_token.as_deref(), Some("google-next"));

    let onedrive = parse_onedrive_list_page(
        r#"{"value":[{"name":"r2.redb.enc","file":{}},{"name":"folder","folder":{}}],"@odata.nextLink":"https://graph.example/next?$skiptoken=abc"}"#,
    )
    .expect("OneDrive page");
    assert_eq!(onedrive.names, vec!["r2.redb.enc"]);
    assert_eq!(
        onedrive.next_link.as_deref(),
        Some("https://graph.example/next?$skiptoken=abc")
    );

    let aliyun = parse_aliyun_drive_list_page(&serde_json::json!({
        "items": [
            {"name": "r3.redb.enc", "type": "file"},
            {"name": "folder", "type": "folder"}
        ],
        "next_marker": "aliyun-next"
    }));
    assert_eq!(aliyun.names, vec!["r3.redb.enc"]);
    assert_eq!(aliyun.next_marker, "aliyun-next");
}

#[test]
fn form_urlencoded_uses_oauth_form_rules() {
    assert_eq!(
        form_urlencoded(&[("client id", "abc+123"), ("secret", "a/b?c")]),
        "client+id=abc%2B123&secret=a%2Fb%3Fc"
    );
}

#[test]
fn onedrive_item_path_joins_root_and_child_segments() {
    assert_eq!(
        onedrive_item_path("/Nya Term/", "/sync/latest.redb"),
        "Nya Term/sync/latest.redb"
    );
    assert_eq!(
        onedrive_item_path("", "sync/latest.redb"),
        "sync/latest.redb"
    );
}

#[test]
fn percent_encode_path_preserves_separators_and_encodes_segments() {
    assert_eq!(
        percent_encode_path("Nya Term/sync/latest redb/猫"),
        "Nya%20Term/sync/latest%20redb/%E7%8C%AB"
    );
}

#[test]
fn onedrive_urls_use_graph_path_addressing_templates() {
    let remote = NativeOneDriveRemote {
        client: zed_reqwest::blocking::Client::builder()
            .build()
            .expect("client"),
        root: "Nya Term".to_string(),
        access_token: Mutex::new("token".to_string().into()),
        refresh_token: None,
        client_id: None,
        client_secret: None,
    };

    assert_eq!(
        remote.children_url("Nya Term/sync"),
        "https://graph.microsoft.com/v1.0/me/drive/root:/Nya%20Term/sync:/children"
    );
    assert_eq!(
        remote.content_url("sync/latest redb").expect("content url"),
        "https://graph.microsoft.com/v1.0/me/drive/root:/Nya%20Term/sync/latest%20redb:/content"
    );
}

#[test]
fn aliyun_drive_type_matches_legacy_values() {
    assert_eq!(
        AliyunDriveType::parse("").expect("default"),
        AliyunDriveType::Default
    );
    assert_eq!(
        AliyunDriveType::parse("resource").expect("resource"),
        AliyunDriveType::Resource
    );
    assert_eq!(
        AliyunDriveType::parse("backup").expect("backup"),
        AliyunDriveType::Backup
    );
    assert!(AliyunDriveType::parse("archive").is_err());
}

#[test]
fn aliyun_drive_item_path_uses_rooted_absolute_path() {
    let remote = NativeAliyunDriveRemote {
        client: zed_reqwest::blocking::Client::builder()
            .build()
            .expect("client"),
        root: "Nya Term".to_string(),
        drive_type: AliyunDriveType::Resource,
        access_token: Mutex::new("token".to_string().into()),
        refresh_token: Mutex::new(nyaterm_core::SecretString::default()),
        client_id: None,
        client_secret: None,
        drive_id: Mutex::new(None),
    };

    assert_eq!(
        remote.item_path("sync/latest redb"),
        "/Nya Term/sync/latest redb"
    );
    assert_eq!(remote.item_path(""), "/Nya Term");
}

#[test]
fn aliyun_drive_error_helpers_preserve_code_and_message() {
    let body = r#"{"code":"NotFound.File","message":"file missing"}"#;

    assert_eq!(
        aliyun_drive_error_code(body).as_deref(),
        Some("NotFound.File")
    );
    assert!(
        aliyun_drive_remote_error(StatusCode::BAD_REQUEST, body, "lookup")
            .to_string()
            .contains("NotFound.File: file missing")
    );
}
