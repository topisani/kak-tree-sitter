//! A simple FIFO wrapper.

use std::{
  ffi::CString,
  fs::{self, File, OpenOptions},
  io::{ErrorKind, Read},
  os::{
    fd::AsRawFd,
    unix::{ffi::OsStrExt, fs::OpenOptionsExt},
  },
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};

use mio::{Interest, Registry, Token, unix::SourceFd};

use crate::error::OhNo;

use super::tokens::Tokens;

/// Result of reading from a fifo.
#[derive(Debug)]
pub enum FifoResult<'a> {
  /// The fifo still misses bytes.
  Unready,

  /// The fifo can be read.
  Ready(ReadyFifo<'a>),
}

impl<'a> FifoResult<'a> {
  /// Get access to the underlying buffer if the fifo is ready.
  pub fn ready(self) -> Option<ReadyFifo<'a>> {
    match self {
      FifoResult::Unready => None,
      FifoResult::Ready(ready) => Some(ready),
    }
  }
}

/// Ready-to-read fifo.
#[derive(Debug)]
pub struct ReadyFifo<'a> {
  /// Start index in the buffer where to read the content.
  start_index: usize,

  /// End index in the buffer where to read the content.
  end_index: usize,

  /// Content.
  buf: &'a mut String,
}

impl ReadyFifo<'_> {
  pub fn as_str(&self) -> &str {
    &self.buf[self.start_index..self.end_index]
  }
}

#[derive(Debug)]
pub struct Fifo {
  registry: Arc<Registry>,
  tokens: Arc<Mutex<Tokens>>,
  path: PathBuf,
  file: File,
  tkn: Token,
  sentinel: String,
  buf: String,
}

impl Fifo {
  /// Create a FIFO.
  pub fn create(
    registry: &Arc<Registry>,
    tokens: &Arc<Mutex<Tokens>>,
    path: impl Into<PathBuf>,
  ) -> Result<Self, OhNo> {
    let path = path.into();

    Self::create_fifo(&path)?;
    let file = Self::open_nonblocking(&path)?;
    let tkn = Self::register(registry, tokens, &file)?;
    let sentinel = uuid::Uuid::new_v4().to_string();

    Ok(Self {
      registry: registry.clone(),
      tokens: tokens.clone(),
      path,
      file,
      tkn,
      sentinel,
      buf: String::default(),
    })
  }

  /// Wrap libc::mkfifo() and create the FIFO on the filesystem within the [`ServerResources`].
  fn create_fifo(path: &Path) -> Result<(), OhNo> {
    // if the file already exists, abort
    if let Ok(true) = path.try_exists() {
      log::debug!("FIFO already exists for path {}", path.display());
      return Ok(());
    }

    let path_bytes = path.as_os_str().as_bytes();
    let c_path = CString::new(path_bytes).map_err(|err| OhNo::CannotCreateFifo {
      err: err.to_string(),
    })?;

    let c_err = unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) };
    if c_err != 0 {
      return Err(OhNo::CannotCreateFifo {
        err: format!("cannot create FIFO at path {path}", path = path.display()),
      });
    }

    Ok(())
  }

  fn open_nonblocking(path: &Path) -> Result<File, OhNo> {
    OpenOptions::new()
      .read(true)
      .custom_flags(libc::O_NONBLOCK)
      .open(path)
      .map_err(|err| OhNo::CannotOpenFifo { err })
  }

  fn register(
    registry: &Arc<Registry>,
    tokens: &Arc<Mutex<Tokens>>,
    file: &File,
  ) -> Result<Token, OhNo> {
    let tkn = tokens.lock().expect("tokens").create();
    registry
      .register(&mut SourceFd(&file.as_raw_fd()), tkn, Interest::READABLE)
      .map_err(|err| OhNo::PollError { err })?;

    Ok(tkn)
  }

  fn unregister(&self) {
    if let Err(err) = self
      .registry
      .deregister(&mut SourceFd(&self.file.as_raw_fd()))
    {
      log::error!(
        "cannot unregister FIFO {path} from poll registry: {err}",
        path = self.path.display()
      );
    }

    self.tokens.lock().expect("tokens").recycle(self.tkn);
  }

  pub fn token(&self) -> Token {
    self.tkn
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn sentinel(&self) -> &str {
    &self.sentinel
  }

  /// Read on the fifo until the buffer can be read.
  pub fn read(&mut self) -> Result<FifoResult<'_>, OhNo> {
    loop {
      match self.file.read_to_string(&mut self.buf) {
        Ok(0) => break, // return to the event loop
        Ok(_) => continue,

        Err(err) => match err.kind() {
          ErrorKind::WouldBlock => break, // return to the event loop

          _ => {
            // reset the buffer in case of errors
            self.buf.clear();
            return Err(OhNo::CannotReadFifo { err });
          }
        },
      }
    }

    // TODO: we can drop that sentinel thing once we have varlen prefixes
    // search for the sentinel; if we find it, it means we have a complete
    // buffer; cut it from the data and reset to be ready to read the next
    // buffer
    let Some(index) = self.buf.find(&self.sentinel) else {
      return Ok(FifoResult::Unready);
    };

    log::trace!(
      "found sentinel {sentinel} in buffer {path}",
      sentinel = self.sentinel,
      path = self.path.display()
    );

    // check that there is no extra data after the buffer; if so, the fifo considered non-ready and
    // we need to drop the data as the new data has higher priority
    let end_index = index + self.sentinel.len();
    if end_index != self.buf.len() {
      log::warn!("receiving buffer updates too fast; dropping current data to prioritize next");
      self.buf.drain(..end_index);
      return Ok(FifoResult::Unready);
    }

    Ok(FifoResult::Ready(ReadyFifo {
      // TODO: we use 0 because we use sentinels, but we should be using variable length prefixes at some
      // point; we will use start_index there
      start_index: 0,
      end_index: index,
      buf: &mut self.buf,
    }))
  }

  /// Clear the associated buffer.
  pub fn clear(&mut self) {
    self.buf.clear();
  }
}

// We implement Drop here because we want to clean the FIFO when the session
// exits automatically. Failing doing so is not a hard error.
impl Drop for Fifo {
  fn drop(&mut self) {
    self.unregister();

    if let Err(err) = fs::remove_file(&self.path) {
      log::warn!("cannot remove FIFO at path {}: {err}", self.path.display());
    }
  }
}
