//! Tripple buffering for faster buffer streaming.
//!
//! This module contains the middle part of a string triple buffer. It is implemented in a way such as
//! writing to the in-between buffer implies copying string slices, but reading is a fast swap.
//!
//! Additionally, this implementation also uses a flag to check whether the reader has read the last update,
//! and if so, it’s possible to prevent the reading part to read twice the same update.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

#[derive(Clone, Debug)]
pub struct TripleBuffer {
  pub writer: TripleBufferWriter,
  pub reader: TripleBufferReader,
}

impl TripleBuffer {
  /// Create a triple-buffer and return its reader and writer part.
  pub fn new() -> Self {
    let buf = Arc::new(Mutex::new(String::new()));
    let was_read = Arc::new(AtomicBool::new(false));

    let writer = TripleBufferWriter {
      buf: buf.clone(),
      was_read: was_read.clone(),
    };
    let reader = TripleBufferReader { buf, was_read };

    Self { writer, reader }
  }
}

#[derive(Clone, Debug)]
pub struct TripleBufferReader {
  buf: Arc<Mutex<String>>,
  was_read: Arc<AtomicBool>,
}

impl TripleBufferReader {
  /// Read the last update of the buffer and return `true`. If we had previously already read, return `false`
  /// and do not update the target buffer.
  pub fn read_to(&self, target: &mut String) -> bool {
    // do not read again if we already read the last buffer update
    if self.was_read.load(std::sync::atomic::Ordering::Relaxed) {
      return false;
    }

    let mut buf = self.buf.lock().unwrap();
    std::mem::swap(&mut *buf, target);
    self
      .was_read
      .store(true, std::sync::atomic::Ordering::Relaxed);
    true
  }
}

#[derive(Clone, Debug)]
pub struct TripleBufferWriter {
  buf: Arc<Mutex<String>>,
  was_read: Arc<AtomicBool>,
}

impl TripleBufferWriter {
  pub fn write(&self, s: &str) {
    let mut buf = self.buf.lock().unwrap();

    buf.clear();
    buf.push_str(s);

    self
      .was_read
      .store(false, std::sync::atomic::Ordering::Relaxed);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn triple_buffer() {
    let tb = TripleBuffer::new();
    let writer = "writer".to_owned();
    let mut reader = String::new();

    let was_read = tb.reader.read_to(&mut reader);
    assert_eq!(reader, "");
    assert!(was_read);

    let was_read = tb.reader.read_to(&mut reader);
    assert!(!was_read);

    tb.writer.write(&writer);
    let was_read = tb.reader.read_to(&mut reader);
    assert_eq!(reader, "writer");
    assert!(was_read);

    let was_read = tb.reader.read_to(&mut reader);
    assert!(!was_read);
  }
}
