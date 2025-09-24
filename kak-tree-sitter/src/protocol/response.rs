//! Response sent from the daemon to Kakoune.

use std::{
  collections::HashSet,
  fmt::{self, Write},
  path::PathBuf,
  rc::Rc,
};

use crate::{kakoune::selection::Sel, tree_sitter::highlighting::KakHighlightRange};

use super::request::Metadata;

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

  pub fn from_req_metadata(metadata: Metadata, payload: Payload) -> Self {
    Self::new(metadata.session, metadata.client, metadata.buffer, payload)
  }

  pub fn session(&self) -> &str {
    &self.session
  }

  pub fn serialize_into(&self, resp_str: &mut String) -> Result<(), fmt::Error> {
    write!(resp_str, "evaluate-commands -no-hooks ")?;

    if let Some(ref buffer) = self.buffer {
      write!(resp_str, "-buffer '{buffer}' ")?;
    } else if let Some(ref client) = self.client {
      write!(resp_str, "-try-client '{client}' ")?;
    }

    write!(resp_str, "-- %[ ")?;
    self.payload.serialize_into(resp_str)?;
    writeln!(resp_str, " ]")?;

    Ok(())
  }
}

/// Response payload.
#[derive(Debug, Eq, PartialEq)]
pub enum Payload {
  /// Initial response when a session starts.
  Init {
    /// Languages that will be recognized by KTS.
    enabled_langs: Vec<EnabledLang>,
  },

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
  Highlights {
    hl_names: Rc<Vec<String>>,
    ranges: Vec<KakHighlightRange>,
  },

  /// Selections.
  ///
  /// These selections are typically returned when the user asked to perform text-objects queries.
  Selections { sels: Vec<Sel> },
}

impl Payload {
  /// Turn the [`Payload`] into a Kakoune command that can be executed remotely.
  pub fn serialize_into(&self, resp_str: &mut String) -> Result<(), fmt::Error> {
    match self {
      Payload::Init { enabled_langs } => {
        for enabled_lang in enabled_langs {
          let name = &enabled_lang.name;

          // logic to run when a buffer sets tree_sitter_lang
          writeln!(
            resp_str,
            "hook -group tree-sitter global WinSetOption tree_sitter_lang={name} %<"
          )?;

          // try to remove the highlighter of the already existing opened buffer
          if enabled_lang.remove_default_highlighter {
            writeln!(resp_str, "try 'remove-highlighter window/{name}'")?;
          }

          writeln!(resp_str, "tree-sitter-buffer-metadata")?;
          writeln!(
            resp_str,
            "add-highlighter -override buffer/tree-sitter-highlighter ranges tree_sitter_hl_ranges"
          )?;
          writeln!(resp_str, "tree-sitter-user-after-highlighter")?;
          writeln!(resp_str, ">")?;

          // automatic config for the language and its aliases
          if enabled_lang.filetype_hook {
            for alias in enabled_lang.aliases.iter().chain(Some(name)) {
              // remove the hook that set a default highlighter
              if enabled_lang.remove_default_highlighter {
                writeln!(resp_str, "try 'remove-hooks global {alias}-highlight'")?;
              }

              // set the alias tree-sitter name to the enabled language `name`
              writeln!(
                resp_str,
                r#"
                  hook -group tree-sitter global BufSetOption filetype={alias} %{{
                    # Forward the filetype as tree-sitter language.
                    set-option buffer tree_sitter_lang {name}
                  }}"#
              )?;
            }
          }
        }

        writeln!(resp_str, "tree-sitter-hook-install-session")?;
        writeln!(resp_str, "tree-sitter-initial-set-buffer-lang")?;
      }

      Payload::Deinit => writeln!(resp_str, "tree-sitter-remove-all")?,

      Payload::BufferSetup {
        fifo_path,
        sentinel,
      } => {
        writeln!(
          resp_str,
          "set-option buffer tree_sitter_buf_fifo_path {}",
          fifo_path.display()
        )?;

        writeln!(
          resp_str,
          "set-option buffer tree_sitter_buf_sentinel {sentinel}"
        )?;

        writeln!(resp_str, "tree-sitter-hook-install-update")?;
      }

      Payload::Highlights { hl_names, ranges } => {
        write!(
          resp_str,
          "set buffer tree_sitter_hl_ranges %val{{timestamp}} "
        )?;

        for range in ranges {
          resp_str.push(' ');
          range.serialize_into(hl_names, resp_str)?;
        }

        writeln!(resp_str)?;
      }

      Payload::Selections { sels } => {
        if sels.is_empty() {
          writeln!(resp_str, "info -title tree-sitter 'no selection'")?;
        } else {
          write!(resp_str, "select ")?;

          for sel in sels {
            resp_str.push(' ');
            sel.serialize_into(resp_str)?;
          }

          writeln!(resp_str)?;
        }
      }
    }

    Ok(())
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
