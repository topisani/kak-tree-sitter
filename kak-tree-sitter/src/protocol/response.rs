//! Response sent from the daemon to Kakoune.

use std::{collections::HashSet, fmt::Write, path::PathBuf, sync::mpsc::Sender};

use itertools::Itertools;

use crate::{kakoune::selection::Sel, tree_sitter::highlighting::KakHighlightRange};

/// Response sent from KTS to Kakoune.
#[derive(Debug, Eq, PartialEq)]
pub struct Response {
  session: String,
  client: Option<String>,
  buffer: Option<String>,
  payload: Payload,
}

impl Response {
  pub fn new(
    session: impl Into<String>,
    client: impl Into<Option<String>>,
    buffer: impl Into<Option<String>>,
    payload: Payload,
  ) -> Self {
    Self {
      session: session.into(),
      client: client.into(),
      buffer: buffer.into(),
      payload,
    }
  }

  pub fn session(&self) -> &str {
    &self.session
  }

  pub fn to_kak(&self) -> Option<String> {
    let payload = self.payload.to_kak();

    // empty payload means no response
    if payload.is_empty() {
      return None;
    }

    let prefix = if let Some(ref buffer) = self.buffer {
      let escaped = buffer.replace(',', "\\,");
      format!("-buffer '{escaped}' ")
    } else if let Some(ref client) = self.client {
      format!("-try-client '{client}' ")
    } else {
      String::new()
    };

    Some(format!(
      "evaluate-commands -no-hooks {prefix} -- %[ {payload} ]"
    ))
  }
}

/// Response payload.
#[derive(Debug, Eq, PartialEq)]
pub enum Payload {
  /// Initial response when a session starts.
  ///
  /// This is a list of (language, remove_default_highlighter) configuration.
  Init { enabled_langs: Vec<EnabledLang> },

  /// Explicit deinit response when the daemon exits.
  ///
  /// This is sent to all connected sessions to ask them to deinit when the server is going down. This is important as
  /// a KTS-enabled session will use various resources (UNIX sockets, FIFOs, etc.) to communicate with KTS, and most of
  /// those will block on Kakoune.
  Deinit,

  /// A buffer metadata changes and the new version is accepted by the server.
  BufferSetup {
    /// FIFO where Kakoune should stream update
    fifo_path: PathBuf,

    /// Sentinel code used to delimit end of buffers inside the FIFO.
    sentinel: String,
  },

  /// Highlights.
  ///
  /// This response is generated when new highlights are available.
  Highlights { ranges: Vec<KakHighlightRange> },

  /// Selections.
  ///
  /// These selections are typically returned when the user asked to perform text-objects queries.
  Selections { sels: Vec<Sel> },
}

impl Payload {
  /// Turn the [`Payload`] into a Kakoune command that can be executed remotely.
  pub fn to_kak(&self) -> String {
    match self {
      Payload::Init { enabled_langs } => {
        let add_hl =
          "add-highlighter -override buffer/tree-sitter-highlighter ranges tree_sitter_hl_ranges";
        let per_lang = enabled_langs
          .iter()
          .map(|enabled_lang| {
            let name = &enabled_lang.name;

            // try to remove the highlighter of the already existing opened buffer
            let remove_default_hl = if enabled_lang.remove_default_highlighter {
              format!("try 'remove-highlighter window/{name}'")
            } else {
              String::new()
            };

            // logic to run when a buffer set tree_sitter_lang
            let mut config = String::new();
            write!(
              &mut config,
              "hook -group tree-sitter global WinSetOption tree_sitter_lang={name} %<
                 {remove_default_hl}
                 tree-sitter-buffer-metadata
                 {add_hl}
                 tree-sitter-user-after-highlighter
               >",
            )
            .unwrap();

            // automatic config for the language and its aliases
            if enabled_lang.filetype_hook {
              for alias in enabled_lang.aliases.iter().chain(Some(name)) {
                // remove the hook that set a default highlighter
                if enabled_lang.remove_default_highlighter {
                  config.push_str(&format!("\ntry 'remove-hooks global {alias}-highlight'"));
                }

                // set the alias tree-sitter name to the enabled language `name`
                write!(
                  &mut config,
                  r#"
                  hook -group tree-sitter global BufSetOption filetype={alias} %{{
                    # Forward the filetype as tree-sitter language.
                    set-option buffer tree_sitter_lang {name}
                  }}"#
                )
                .unwrap();
              }
            }

            config
          })
          .join("\n");

        [
          per_lang,
          "tree-sitter-hook-install-session".to_owned(),
          "tree-sitter-initial-set-buffer-lang".to_owned(),
        ]
        .join("\n")
      }

      Payload::Deinit => "tree-sitter-remove-all".to_owned(),

      Payload::BufferSetup {
        fifo_path,
        sentinel,
      } => [
        format!(
          "set-option buffer tree_sitter_buf_fifo_path {}",
          fifo_path.display()
        ),
        format!("set-option buffer tree_sitter_buf_sentinel {sentinel}"),
        "tree-sitter-hook-install-update".to_owned(),
      ]
      .into_iter()
      .filter(|s| !s.is_empty())
      .join("\n"),

      Payload::Highlights { ranges } => {
        let ranges_str = ranges
          .iter()
          .map(KakHighlightRange::to_kak_range_str)
          .join(" ");

        format!(
          "{range_specs} %val{{timestamp}} {ranges_str}",
          range_specs = "set buffer tree_sitter_hl_ranges",
        )
      }

      Payload::Selections { sels } => {
        let sels_str = sels.iter().map(|sel| sel.to_kak_str()).join(" ");
        format!("select {sels_str}")
      }
    }
  }
}

/// Add replies to the response queue.
///
/// Response are not immediately sent back to Kakoune, but instead enqueued into
/// the response queue. That is required so that we do not block while sending.
#[derive(Clone, Debug)]
pub struct EnqueueResponse {
  sender: Sender<Response>,
}

impl EnqueueResponse {
  /// Create a new [`ResponseSender`].
  pub fn new(sender: Sender<Response>) -> Self {
    Self { sender }
  }

  /// Enqueue a response.
  ///
  /// That function never fails as it logs any underlying errors.
  pub fn enqueue(&self, resp: Response) {
    if let Err(err) = self.sender.send(resp) {
      log::error!("cannot send response: {err}");
    }
  }
}

/// Tree-sitter enabled language.
///
/// This type contains information to enable support for a language.
#[derive(Debug, Eq, PartialEq)]
pub struct EnabledLang {
  pub name: String,
  pub remove_default_highlighter: bool,
  pub filetype_hook: bool,
  pub aliases: HashSet<String>,
}
