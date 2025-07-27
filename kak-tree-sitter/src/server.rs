pub mod fifo;
pub mod handler;
pub mod io;
pub mod resources;
mod tokens;

use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

use io::IOHandler;
use kak_tree_sitter_config::Config;
use mio::Poll;

use crate::{
  cli::Cli,
  error::OhNo,
  kakoune::session::{Session, SessionTracker},
  protocol::{
    request::Request,
    response::{self, Response},
  },
};

use self::resources::{Paths, ServerResources};

pub struct Server {
  io_handler: IOHandler,
  session_tracker: SessionTracker,
}

impl Server {
  pub fn new(
    config: &Config,
    cli: &Cli,
    resources: ServerResources,
    poll: Poll,
  ) -> Result<Self, OhNo> {
    log::debug!(
      "starting server on socket at {}",
      resources.paths().socket_path().display()
    );

    log::debug!("creating session tracker");
    let session_tracker = SessionTracker::default();

    log::debug!("creating IO handler");
    let io_handler = IOHandler::new(
      config,
      cli.is_standalone(),
      cli.with_highlighting || config.features.highlighting,
      resources,
      poll,
    )?;

    Ok(Server {
      io_handler,
      session_tracker,
    })
  }

  pub fn is_server_running(paths: &Paths) -> bool {
    match std::fs::read_to_string(paths.pid_path()) {
      Err(_) => false,
      Ok(pid) => ServerResources::is_running(pid.trim()),
    }
  }

  /// Initiate the first session, if any.
  ///
  /// It’s possible to start the server from within Kakoune. In that case, we
  /// need to simulate an init request from that session.
  pub fn init_first_session(&mut self, session: impl Into<String>) -> Result<(), OhNo> {
    let session = session.into();
    log::info!("initiating first session {session}");

    self
      .io_handler
      .process_req(&mut self.session_tracker, Request::init_session(session))?;

    Ok(())
  }

  /// Start the server state and wait for events to be dispatched.
  pub fn start(mut self) -> Result<(), OhNo> {
    log::info!("starting server");

    let quit = Arc::new(AtomicBool::new(false));
    let waker = self.io_handler.waker()?;

    {
      let quit = quit.clone();
      ctrlc::set_handler(move || {
        log::debug!("SIGINT received");
        quit.store(true, Ordering::Relaxed);

        if let Err(err) = waker.wake() {
          log::error!("cannot wake poll: {err}");
        }
      })?;
    }

    self
      .io_handler
      .start(&mut self.session_tracker, quit.clone());

    log::info!("shutting down");
    self.disconnect_sessions();

    Ok(())
  }

  /// Disconnect all sessions by sending them all a [`Response::Deinit`].
  fn disconnect_sessions(&self) {
    for session_name in self.session_tracker.sessions() {
      let resp = Response::new(session_name, None, None, response::Payload::Deinit);
      if let Err(err) = Session::send_response(resp) {
        log::error!("error while sending disconnect: {err}");
      }
    }
  }
}
