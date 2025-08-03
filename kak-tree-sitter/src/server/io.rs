use std::{
  collections::{HashMap, hash_map},
  io::Read as _,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, Sender, channel},
  },
  time::Duration,
};

use kak_tree_sitter_config::Config;
use mio::{
  Interest, Poll, Token, Waker,
  net::{UnixListener, UnixStream},
};

use crate::{
  error::OhNo,
  kakoune::{
    buffer::BufferId,
    selection::Sel,
    session::{Session, SessionTracker},
  },
  protocol::request::{self, Metadata, Request},
  server::handler::Command,
};

use super::{
  fifo::Fifo,
  handler::{CommandSender, Handler},
  resources::ServerResources,
};

/// Feedback provided after a request has finished. Mainly used to shutdown.
#[derive(Debug)]
pub enum Feedback {
  Ok,
  ShouldExit,
}

/// Send a buffer back from the handler to the async IO handler.
///
/// When the async IO hander computes a new buffer to be sent to the handler via a command, it expects
/// the hander to send back a buffer string. This is an optimization to prevent allocating all over the place
/// and allow for reusing previous strings.
#[derive(Clone, Debug)]
pub struct BackBuffer {
  sender: Sender<String>,
}

impl BackBuffer {
  pub fn new(sender: Sender<String>) -> Self {
    Self { sender }
  }

  pub fn send_back(self, s: String) {
    if let Err(err) = self.sender.send(s) {
      log::warn!("cannot send back buffer: {err}");
    }
  }
}

/// Async IO handler.
///
/// This type is responsible for reading both Unix socket and FIFO buffer content as fast as possible,
/// creating requests based on these, and forwarding the requests to the actual [`ReqHander`].
pub struct IOHandler {
  is_standalone: bool,
  with_highlighting: bool,
  resources: ServerResources,
  fifos: HashMap<Token, (Metadata, Fifo)>,
  tkn_buffer_ids: HashMap<BufferId, Token>,
  poll: Poll,
  unix_listener: UnixListener,
  connections: HashMap<Token, BufferedClient>,
  command_sender: CommandSender,

  // A small pre-allocated strings to send buffers as [`Command`]. Prevents allocating more strings.
  buffer_strs: Vec<String>,
  buffer_receiver: Receiver<String>,
  back_buffer: BackBuffer,
}

impl IOHandler {
  const WAKE_TKN: Token = Token(0);
  const UNIX_LISTENER_TKN: Token = Token(1);

  pub fn new(
    config: &Config,
    is_standalone: bool,
    with_highlighting: bool,
    resources: ServerResources,
    poll: Poll,
  ) -> Result<Self, OhNo> {
    let mut unix_listener = UnixListener::bind(resources.paths().socket_path())
      .map_err(|err| OhNo::CannotStartServer { err })?;
    let connections = HashMap::default();
    let fifos = HashMap::new();
    let tkn_buffer_ids = HashMap::new();

    poll
      .registry()
      .register(
        &mut unix_listener,
        Self::UNIX_LISTENER_TKN,
        Interest::READABLE,
      )
      .map_err(|err| OhNo::PollError { err })?;

    let command_sender = Handler::create(config, with_highlighting);
    let (back_buffer_sender, buffer_receiver) = channel();
    let back_buffer = BackBuffer::new(back_buffer_sender);

    Ok(Self {
      is_standalone,
      with_highlighting,
      resources,
      fifos,
      tkn_buffer_ids,
      poll,
      unix_listener,
      connections,
      command_sender,
      buffer_strs: Vec::new(),
      buffer_receiver,
      back_buffer,
    })
  }

  pub fn start(&mut self, session_tracker: &mut SessionTracker, quit: Arc<AtomicBool>) {
    let mut events = mio::Events::with_capacity(64);

    log::debug!("starting event loop");
    'event_loop: loop {
      match self.poll.poll(&mut events, Some(Duration::from_secs(1))) {
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,

        Err(err) => {
          log::error!("error while polling: {err}");
          break;
        }

        _ => (),
      }

      if quit.load(Ordering::Relaxed) {
        break 'event_loop;
      }

      for ev in &events {
        match ev.token() {
          Self::UNIX_LISTENER_TKN if ev.is_readable() => {
            if let Err(err) = self.unix_listener_accept() {
              log::error!("error while accepting UNIX connection: {err}");
            }
          }

          tkn if ev.is_readable() => {
            if let Feedback::ShouldExit = self.dispatch_read_token(session_tracker, tkn) {
              break 'event_loop;
            }
          }

          _ => (),
        }
      }
    }

    log::debug!("poll loop exited");
  }

  pub fn waker(&self) -> Result<Arc<Waker>, OhNo> {
    let waker =
      Waker::new(self.poll.registry(), Self::WAKE_TKN).map_err(|err| OhNo::PollError { err })?;
    Ok(Arc::new(waker))
  }

  fn unix_listener_accept(&mut self) -> Result<(), OhNo> {
    loop {
      let (mut client, _) = match self.unix_listener.accept() {
        Ok(conn) => conn,
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
        Err(err) => return Err(OhNo::UnixSocketConnectionError { err }),
      };

      log::debug!("client connected: {client:?}");
      let token = self.resources.tokens().lock().expect("tokens").create();
      let res = self
        .poll
        .registry()
        .register(&mut client, token, Interest::READABLE)
        .map_err(|err| OhNo::PollError { err });

      if let Err(err) = res {
        self
          .resources
          .tokens()
          .lock()
          .expect("tokens")
          .recycle(token);

        return Err(err);
      }

      log::debug!("{client:?} will be using token {token:?}");
      self.connections.insert(token, BufferedClient::new(client));
    }

    Ok(())
  }

  /// Find which object is behind the input token and perform a read action on it.
  fn dispatch_read_token(
    &mut self,
    session_tracker: &mut SessionTracker,
    token: Token,
  ) -> Feedback {
    match self.read_unix_client(session_tracker, token) {
      Ok(Some(feedback)) => return feedback,

      Err(err) => {
        log::error!("error while reading from UNIX client (token = {token:?}): {err}");
        return Feedback::Ok;
      }

      _ => (),
    }

    if let Err(err) = self.read_buffer(token) {
      log::error!("error while reading buffer: (token = {token:?}): {err}");
    }

    Feedback::Ok
  }

  /// Try to read from a (connected) UNIX client.
  ///
  /// Return `false` if the token is not for a UNIX client.
  fn read_unix_client(
    &mut self,
    session_tracker: &mut SessionTracker,
    tkn: Token,
  ) -> Result<Option<Feedback>, OhNo> {
    let Some(client) = self.connections.get_mut(&tkn) else {
      return Ok(None);
    };

    // read the client request; exit and get back to the polling loop if not complete yet
    let Some(s) = client.read()? else {
      return Ok(None);
    };

    let req = Request::from_json(s)?;
    self.process_req(session_tracker, req).map(Some)
  }

  pub fn process_req(
    &mut self,
    session_tracker: &mut SessionTracker,
    req: Request,
  ) -> Result<Feedback, OhNo> {
    match req.payload {
      request::Payload::SessionBegin => {
        let session = req.session();
        if session_tracker.tracks(session) {
          log::warn!("session {session} already tracked");
          return Ok(Feedback::Ok);
        }

        log::info!("registering session {}", req.session());

        let session = Session::new(req.session())?;
        session_tracker.track(session);

        self.command_sender.send(Command::SessionInit {
          metadata: req.metadata,
        })?;
      }

      request::Payload::SessionEnd => {
        log::info!("session {} exit", req.session());

        self.command_sender.send(Command::SessionEnd {
          metadata: req.metadata.clone(),
        })?;

        session_tracker.untrack(req.session());

        // only shutdown if were started with an initial session (non standalone)
        let feedback = if !self.is_standalone && session_tracker.is_empty() {
          log::info!("last session exited; stopping the server…");
          Feedback::ShouldExit
        } else {
          Feedback::Ok
        };

        return Ok(feedback);
      }

      request::Payload::Reload => {
        log::info!("reloading configuration, grammars and queries");
        self.reload();
      }

      request::Payload::Shutdown => {
        log::info!("shutting down");
        self.command_sender.send(Command::Shutdown)?;
        return Ok(Feedback::ShouldExit);
      }

      request::Payload::BufferMetadata { lang } => {
        let metadata = req.metadata;
        let id = metadata.to_buffer_id()?;
        log::info!("buffer metadata {metadata:?} ({lang})");

        // ensure we have a fifo for this buffer; if not, create one
        let (fifo_path, sentinel) = match self.tkn_buffer_ids.entry(id.clone()) {
          hash_map::Entry::Occupied(entry) => {
            let tkn = *entry.get();
            let (_, fifo) = self
              .fifos
              .get(&tkn)
              .ok_or_else(|| OhNo::UnknownToken { tkn })?;
            (fifo.path().to_owned(), fifo.sentinel().to_owned())
          }

          hash_map::Entry::Vacant(entry) => {
            // create a new fifo associated with a token if none exists
            let fifo = self.resources.new_fifo()?;
            let tkn = fifo.token();
            let ret = (fifo.path().to_owned(), fifo.sentinel().to_owned());

            entry.insert(tkn);
            self.fifos.insert(tkn, (metadata.clone(), fifo));

            ret
          }
        };

        self.command_sender.send(Command::BufferMetadata {
          metadata,
          lang,
          fifo_path,
          sentinel,
        })?;
      }

      request::Payload::BufferClose => {
        let metadata = req.metadata;
        let id = metadata.to_buffer_id()?;
        log::info!("buffer close {metadata:?}");

        // remove the fifo and reverse lookup; the fifo content is cleaned up on drop
        if let Some(tkn) = self.tkn_buffer_ids.remove(&id) {
          self.fifos.remove(&tkn);
        }

        self
          .command_sender
          .send(Command::BufferClose { metadata })?;
      }

      request::Payload::TextObjects {
        pattern,
        selections,
        mode,
      } => {
        let metadata = req.metadata;
        let selections = Sel::parse_many(&selections);
        log::info!(
          "text objects for {metadata:?}, pattern {pattern}, mode {mode:?}, selections: {selections:?}"
        );

        self.command_sender.send(Command::TextObjects {
          metadata,
          pattern,
          selections,
          mode,
        })?;
      }

      request::Payload::Nav { selections, dir } => {
        let metadata = req.metadata;
        log::info!("nav for buffer {metadata:?}, dir {dir:?}",);

        let selections = Sel::parse_many(&selections);

        self.command_sender.send(Command::Nav {
          metadata,
          selections,
          dir,
        })?;
      }
    }

    Ok(Feedback::Ok)
  }

  /// Read the buffer associated with the argument token.
  fn read_buffer(&mut self, tkn: Token) -> Result<(), OhNo> {
    let Some((metadata, fifo)) = self.fifos.get_mut(&tkn) else {
      return Err(OhNo::UnknownToken { tkn });
    };

    let Some(ready_fifo) = fifo.read()?.ready() else {
      // return to the event loop
      return Ok(());
    };

    // grab a buffer string; we start with available buffer string; if none exists, we try to get one
    // from the back buffer channel; if none is present, we allocate one with the buffer length
    let mut buf = self
      .buffer_strs
      .pop()
      .or_else(|| self.buffer_receiver.try_recv().ok())
      .unwrap_or_else(|| String::with_capacity(ready_fifo.len()));

    ready_fifo.copy_to(&mut buf);
    fifo.clear();
    self.command_sender.send(Command::BufferUpdate {
      metadata: metadata.clone(),
      buf,
      back_buffer_sender: self.back_buffer.clone(),
    })?;

    Ok(())
  }

  fn reload(&mut self) {
    let config = match Config::load_from_xdg() {
      Ok(config) => config,
      Err(err) => {
        log::error!("reloading config failed: {err}");
        return;
      }
    };

    self.command_sender = Handler::create(&config, self.with_highlighting);
  }
}

/// UNIX socket client with associated buffer.
pub struct BufferedClient {
  client: UnixStream,
  buf: String,
}

impl BufferedClient {
  pub fn new(client: UnixStream) -> Self {
    Self {
      client,
      buf: String::default(),
    }
  }

  pub fn read(&mut self) -> Result<Option<&str>, OhNo> {
    loop {
      match self.client.read_to_string(&mut self.buf) {
        Ok(0) => return Ok(Some(self.buf.as_str())),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
        Err(err) => return Err(OhNo::UnixSocketReadError { err }),
        _ => continue,
      }
    }
  }
}
