//! Requests that can be sent to the server from Kakoune.

use serde::{Deserialize, Serialize};

use crate::{
  error::OhNo,
  kakoune::{buffer::BufferId, text_objects::OperationMode},
  tree_sitter::nav,
};

/// Metadata associated with the request.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct Metadata {
  pub session: String,
  pub client: Option<String>,
  pub buffer: Option<String>,
}

impl Metadata {
  fn new(session: impl Into<String>) -> Self {
    Self {
      session: session.into(),
      client: None,
      buffer: None,
    }
  }

  pub fn to_buffer_id(&self) -> Result<BufferId, OhNo> {
    let buffer = self.buffer.clone().ok_or_else(|| OhNo::UnknownBuffer {
      id: BufferId::new(self.session.clone(), String::new()),
    })?;

    Ok(BufferId::new(self.session.clone(), buffer))
  }
}

/// Request.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct Request {
  pub metadata: Metadata,
  pub payload: Payload,
}

impl Request {
  /// Parse a [`Request`] from a JSON string.
  pub fn from_json(s: impl AsRef<str>) -> Result<Self, OhNo> {
    let s = s.as_ref();
    serde_json::from_str(s).map_err(|err| OhNo::InvalidRequest {
      req: s.to_owned(),
      err: err.to_string(),
    })
  }

  pub fn init_session(session: impl Into<String>) -> Self {
    Self {
      metadata: Metadata::new(session),
      payload: Payload::SessionBegin,
    }
  }

  pub fn session(&self) -> &str {
    &self.metadata.session
  }
}

/// Request payload.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
  /// Inform the server that a session exists and that we should be sending back
  /// the Kakoune commands to get the server features.
  SessionBegin,

  /// Inform the server that a session has exited.
  SessionEnd,

  /// Ask the server to reload its configuration and reload grammars / queries.
  Reload,

  /// Ask the server to shutdown.
  Shutdown,

  /// Buffer metadata.
  ///
  /// This should be sent every time the buffer changes (lang, mostly).
  BufferMetadata { lang: String },

  /// Buffer close.
  BufferClose,

  /// Request to apply text-objects on selections.
  TextObjects {
    pattern: String,
    selections: String,
    mode: OperationMode,
  },

  /// Request to navigate the tree-sitter tree on selections.
  Nav { selections: String, dir: nav::Dir },
}
