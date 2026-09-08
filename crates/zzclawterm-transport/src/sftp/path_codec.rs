//! Encoding and decoding of raw SFTP path tokens.

use encoding_rs::{Encoding, GB18030, GBK, UTF_8};

use super::super::SshSessionConfig;

#[derive(Clone, Copy, Debug)]
pub struct SftpPathCodec {
    encoding_name: &'static str,
    encoding: &'static Encoding,
}

impl SftpPathCodec {
    pub fn from_ssh_config(config: &SshSessionConfig) -> anyhow::Result<Self> {
        let requested = config.sftp.filename_encoding.trim();
        let effective = if requested.is_empty() || requested.eq_ignore_ascii_case("terminal") {
            config.encoding.trim()
        } else {
            requested
        };
        Self::from_encoding_name(effective)
    }

    pub fn from_encoding_name(encoding: &str) -> anyhow::Result<Self> {
        let normalized = encoding.trim();
        if normalized.is_empty()
            || normalized.eq_ignore_ascii_case("global")
            || normalized.eq_ignore_ascii_case("terminal")
            || normalized.eq_ignore_ascii_case("utf8")
            || normalized.eq_ignore_ascii_case("utf-8")
        {
            return Ok(Self {
                encoding_name: "UTF-8",
                encoding: UTF_8,
            });
        }
        if normalized.eq_ignore_ascii_case("gbk") || normalized.eq_ignore_ascii_case("gb2312") {
            return Ok(Self {
                encoding_name: "GBK",
                encoding: GBK,
            });
        }
        if normalized.eq_ignore_ascii_case("gb18030") {
            return Ok(Self {
                encoding_name: "GB18030",
                encoding: GB18030,
            });
        }
        anyhow::bail!("Unsupported SFTP filename encoding: {normalized}");
    }

    #[cfg(test)]
    pub fn encoding_name(&self) -> &'static str {
        self.encoding_name
    }

    pub fn encode_path(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let (encoded, _, had_errors) = self.encoding.encode(path);
        if had_errors {
            anyhow::bail!(
                "SFTP path cannot be encoded as {}: {path}",
                self.encoding_name
            );
        }
        Ok(encoded.into_owned())
    }

    #[cfg(test)]
    pub fn decode_path(&self, path: &[u8]) -> anyhow::Result<String> {
        let (decoded, _, had_errors) = self.encoding.decode(path);
        if had_errors {
            anyhow::bail!("SFTP path cannot be decoded as {}", self.encoding_name);
        }
        Ok(decoded.into_owned())
    }

    pub fn decode_path_lossy(&self, path: &[u8]) -> String {
        let (decoded, _, had_errors) = self.encoding.decode(path);
        if had_errors {
            String::from_utf8_lossy(path).into_owned()
        } else {
            decoded.into_owned()
        }
    }
}
