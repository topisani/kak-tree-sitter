mod cli;
mod client;
mod error;
mod kakoune;
mod logging;
mod protocol;
mod server;
mod tree_sitter;

use std::{fs::File, path::Path, sync::Arc};

use clap::Parser;
use cli::Cli;
use error::OhNo;
use kak_tree_sitter_config::Config;
use logging::Verbosity;
use mio::Poll;
use protocol::request::Request;
use server::{
  resources::{Paths, ServerResources},
  Server,
};

use crate::{kakoune::rc, logging::KakouneLogger};

fn main() {
  if let Err(err) = start() {
    log::error!("fatal error: {err}");
    std::process::exit(1);
  }
}

fn setup_logger(cli: &Cli) -> Result<(), OhNo> {
  // if we start from kakoune, the logger implementation sends to the *debug* buffer
  if let Some(level) = Verbosity::from_count(cli.verbose).to_level() {
    if cli.kakoune {
      KakouneLogger::new(level).register()?;
    } else {
      simple_logger::init_with_level(level)?;
    }
  }

  Ok(())
}

fn print_rc(cli: &Cli, config: &Config) {
  println!("{}", rc::static_kak());

  if cli.with_text_objects || config.features.text_objects {
    println!("{}", rc::text_objects_kak());
  }

  println!("{}", rc::cli_args_opt_kak(cli));
}

fn handle_cli_request(cli: &Cli, paths: &Paths, req: &str) -> Result<(), OhNo> {
  // otherwise, regular client
  let req = Request::from_json(req)?;

  let mut client = client::Client::connect(paths)?;
  client.send(&req)?;

  // if we sent the request from within Kakoune, we return nop command so that we can call the commands and print
  // errors along the way
  if cli.kakoune {
    println!("nop");
  }

  Ok(())
}

fn start_server(cli: Cli, config: &Config, paths: Paths) -> Result<(), OhNo> {
  if Server::is_server_running(&paths) {
    log::debug!("server already running");

    if let Some(session) = cli.init {
      log::debug!("initiating first session {session}");
      client::Client::init_session(&paths, session)?;
    }

    return Ok(());
  }

  persist_process(&paths, cli.daemonize)?;

  let poll = Poll::new().map_err(|err| OhNo::PollError { err })?;

  // whatever we do, we will need to know about where the resources are; this
  // object ensures the resources are created and expose interfaces to them
  let registry = Arc::new(
    poll
      .registry()
      .try_clone()
      .map_err(|err| OhNo::PollError { err })?,
  );
  let resources = ServerResources::new(paths, registry)?;
  let mut server = Server::new(config, &cli, resources, poll)?;

  if let Some(session) = cli.init {
    server.init_first_session(session)?;
  }

  server.start()
}

fn start() -> Result<(), OhNo> {
  let cli = Cli::parse();

  setup_logger(&cli)?;

  let config = if let Some(path_config) = &cli.config {
    Config::load_user(path_config)?
  } else {
    Config::load_from_xdg()?
  };

  // inject rc if we start from Kakoune
  if cli.kakoune && cli.init.is_some() {
    print_rc(&cli, &config);
  }

  let paths = Paths::new()?;

  if let Some(ref req) = cli.request {
    return handle_cli_request(&cli, &paths, req);
  }

  if cli.server {
    return start_server(cli, &config, paths);
  }

  Err(OhNo::NothingToDo)
}

/// Create the PID file from the current process, or the one of the child
/// process if daemonized.
fn persist_process(paths: &Paths, daemonize: bool) -> Result<(), OhNo> {
  let pid_file = paths.pid_path();

  // check whether a pid file exists; remove it if any
  if let Ok(true) = pid_file.try_exists() {
    log::debug!("removing previous PID file");
    std::fs::remove_file(&pid_file).map_err(|err| OhNo::CannotStartDaemon {
      err: format!(
        "cannot remove previous PID file {path}: {err}",
        path = pid_file.display()
      ),
    })?;

    log::debug!("removing previous socket file");
    let socket_path = paths.socket_path();

    if let Ok(true) = socket_path.try_exists() {
      if let Err(err) = std::fs::remove_file(&socket_path) {
        return Err(OhNo::CannotStartDaemon {
          err: format!(
            "cannot remove previous socket file {path}: {err}",
            path = socket_path.display()
          ),
        });
      }
    }
  }

  if daemonize {
    persist_daemon(paths, &pid_file)?;
  } else {
    std::fs::write(&pid_file, format!("{}", std::process::id())).map_err(|err| {
      OhNo::CannotWriteFile {
        file: pid_file,
        err,
      }
    })?;
  }

  Ok(())
}

fn persist_daemon(paths: &Paths, pid_file: &Path) -> Result<(), OhNo> {
  // create stdout / stderr files
  let stdout_path = paths.stdout();
  let stderr_path = paths.stderr();
  let stdout = File::create(&stdout_path).map_err(|err| OhNo::CannotCreateFile {
    file: stdout_path,
    err,
  })?;
  let stderr = File::create(&stderr_path).map_err(|err| OhNo::CannotCreateFile {
    file: stderr_path,
    err,
  })?;

  daemonize::Daemonize::new()
    .stdout(stdout)
    .stderr(stderr)
    .pid_file(pid_file)
    .start()
    .map_err(|err| OhNo::CannotStartDaemon {
      err: err.to_string(),
    })?;

  Ok(())
}
