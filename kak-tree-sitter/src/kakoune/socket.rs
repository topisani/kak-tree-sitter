//! Direct Unix socket implementation.
//!
//! This module allows to send commands to Kakoune by directly writing to a Unix
//! socket (instead of the typical `kak -p` process invocation).

use std::{io::Write as _, os::unix::net::UnixStream, path::PathBuf};

use crate::{error::OhNo, protocol::response::Response};

#[derive(Debug)]
pub struct Connection {
  socket: UnixStream,
}

impl Connection {
  pub fn connect(session: impl AsRef<str>) -> Result<Self, OhNo> {
    let path = Self::socket_path(session.as_ref())?;
    let socket = UnixStream::connect(path).map_err(|err| OhNo::KakouneUnixSocketError { err })?;

    Ok(Self { socket })
  }

  pub fn send(&mut self, resp: Response) -> Result<(), OhNo> {
    let Some(s) = resp.to_kak() else {
      return Ok(());
    };
    let bytes = s.as_bytes();

    // content is encoded length + raw message
    let mut content = Vec::new();
    content.extend(encode_len(bytes.len()));
    content.extend(bytes);

    // header is magic byte + length of content
    let mut message = vec![0x02];
    message.extend(encode_len(content.len() + 5));

    message.extend(content);

    self
      .socket
      .write_all(&message)
      .map_err(|err| OhNo::KakouneUnixSocketError { err })
  }

  fn socket_path(session: &str) -> Result<PathBuf, OhNo> {
    let session_dir = dirs::runtime_dir()
      .or_else(|| std::env::var("TMPDIR").ok().map(PathBuf::from))
      .ok_or(OhNo::NoRuntimeDir)?;

    Ok(session_dir.join(format!("kakoune/{session}")))
  }
}

fn encode_len(size: usize) -> [u8; 4] {
  (size as u32).to_ne_bytes()
}
