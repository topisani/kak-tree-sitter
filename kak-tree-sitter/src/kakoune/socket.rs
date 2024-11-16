//! Direct Unix socket implementation.
//!
//! This module allows to send commands to Kakoune by directly writing to a Unix
//! socket (instead of the typical `kak -p` process invocation).

use std::{io::Write, os::unix::net::UnixStream, path::PathBuf};

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

    self
      .socket
      .write_all(s.as_bytes())
      .map_err(|err| OhNo::KakouneUnixSocketError { err })
  }

  fn socket_path(session: &str) -> Result<PathBuf, OhNo> {
    let session_dir = dirs::runtime_dir()
      .or_else(|| std::env::var("TMPDIR").ok().map(PathBuf::from))
      .ok_or(OhNo::NoRuntimeDir)?;

    Ok(session_dir.join(format!("kakoune/{session}")))
  }
}
