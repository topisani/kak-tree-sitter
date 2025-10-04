//! Convert from tree-sitter-highlight events to Kakoune ranges highlighter.

use std::fmt;

use unicode_segmentation::UnicodeSegmentation;

use crate::{kakoune::face::Face, tree_sitter::languages::Languages};

#[derive(Debug, Eq, PartialEq)]
struct FaceId {
  id: usize,
}

impl FaceId {
  fn new(id: usize) -> Self {
    Self { id }
  }
}

/// A convenient representation of a single highlight range for Kakoune.
///
/// `:doc highlighters`, `ranges`, for further documentation.
#[derive(Debug, Eq, PartialEq)]
pub struct KakHighlightRange {
  line_start: usize,
  col_byte_start: usize,
  line_end: usize,
  col_byte_end: usize,
  face: FaceId,
}

impl KakHighlightRange {
  fn new(
    line_start: usize,
    col_byte_start: usize,
    line_end: usize,
    col_byte_end: usize,
    face: FaceId,
  ) -> Self {
    Self {
      line_start,
      col_byte_start,
      line_end,
      col_byte_end,
      face,
    }
  }

  pub fn from_tree_house<'a, 'b: 'a>(
    source: &str,
    mut highlighter: tree_house::highlighter::Highlighter<'a, 'b, Languages>,
  ) -> Vec<Self> {
    let mut kak_hls = Vec::new();
    let mut tree_house_hls = Vec::new();
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    // (byte, line, column)
    let mut last_position = (0, 0, 0);

    loop {
      let offset = highlighter.next_event_offset();
      if offset == u32::MAX {
        break;
      }

      mapper.advance(offset as _);
      let line = mapper.line();
      let col = mapper.col_byte();

      let (event, hl_list) = highlighter.advance();

      if offset == last_position.0 {
        continue;
      }

      match event {
        // reset highlights
        tree_house::highlighter::HighlightEvent::Refresh => {
          tree_house_hls.clear();
          tree_house_hls.extend(hl_list);
        }

        // stack up highlights
        tree_house::highlighter::HighlightEvent::Push => tree_house_hls.extend(hl_list),
      }

      if !tree_house_hls.is_empty() {
        for hl in &tree_house_hls {
          kak_hls.push(KakHighlightRange::new(
            last_position.1,
            last_position.2,
            line,
            col,
            FaceId::new(hl.idx()),
          ));
        }
      }

      last_position = (offset, line, col);
    }

    kak_hls
  }

  /// Display as a string recognized by the `ranges` Kakoune highlighter.
  pub fn serialize_into(
    &self,
    faces: &[Face],
    output: &mut impl fmt::Write,
  ) -> Result<(), fmt::Error> {
    write!(
      output,
      "{}.{},{}.{}|{}",
      self.line_start,
      self.col_byte_start + 1, // range-specs is 1-indexed
      self.line_end,
      self.col_byte_end + 1, // ditto
      faces[self.face.id]
    )
  }
}

/// Map byte indices to line and column.
#[derive(Debug)]
struct ByteLineColMapper<C> {
  chars: C,
  byte_idx: usize,
  line: usize,
  col_byte: usize,
}

impl<'a, C> ByteLineColMapper<C>
where
  C: Iterator<Item = &'a str>,
{
  fn new(chars: C) -> Self {
    Self {
      chars,
      byte_idx: 0,
      line: 1,
      col_byte: 0,
    }
  }

  fn line(&self) -> usize {
    self.line
  }

  fn col_byte(&self) -> usize {
    self.col_byte
  }

  fn should_change_line(s: &str) -> bool {
    ["\n", "\r\n"].contains(&s)
  }

  /// Advance the mapper until the given byte is read (or just passed over).
  fn advance(&mut self, til: usize) {
    loop {
      if self.byte_idx >= til {
        break;
      }

      if let Some(grapheme) = self.chars.next() {
        let bytes = grapheme.len();
        self.byte_idx += bytes;

        if Self::should_change_line(grapheme) {
          self.line += 1;
          self.col_byte = 0;
        } else {
          self.col_byte += bytes;
        }
      } else {
        break;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{cmp::Reverse, time::Duration};

  use ropey::RopeSlice;
  use tree_house::Syntax;
  use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};
  use unicode_segmentation::UnicodeSegmentation;

  use super::ByteLineColMapper;

  #[test]
  fn idempotent_mapper() {
    let source = "Hello, world!";
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 0);

    mapper.advance(1);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 1);
    mapper.advance(1);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 1);

    mapper.advance(4);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 4);
    mapper.advance(4);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 4);
  }

  #[test]
  fn lines_mapper() {
    let source = "const x: &'str = \"Hello, world!\";\nconst y = 3;";
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 0);

    mapper.advance(4);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 4);

    mapper.advance(33);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 33);

    mapper.advance(34);
    assert_eq!(mapper.line(), 2);
    assert_eq!(mapper.col_byte(), 0);

    mapper.advance(35);
    assert_eq!(mapper.line(), 2);
    assert_eq!(mapper.col_byte(), 1);
  }

  #[test]
  fn unicode_mapper() {
    let source = "const ᾩ = 1"; // the unicode symbol is 3-bytes
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 0);

    mapper.advance(1);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 1);

    mapper.advance(5);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 5);

    mapper.advance(6);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 6);

    mapper.advance(7);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 9);

    mapper.advance(8);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 9);

    mapper.advance(9);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 9);
  }

  #[test]
  fn unicode_mapper_more() {
    let source = "× a"; // 2 bytes
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    mapper.advance(1);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 2);
  }

  #[test]
  fn newline_mapper() {
    let source = "×\na"; // 2 bytes, 1 byte, 1 byte
    let mut mapper = ByteLineColMapper::new(source.graphemes(true));

    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 0);

    mapper.advance(1);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 2);

    mapper.advance(2);
    assert_eq!(mapper.line(), 1);
    assert_eq!(mapper.col_byte(), 2);

    mapper.advance(3);
    assert_eq!(mapper.line(), 2);
    assert_eq!(mapper.col_byte(), 0);
  }

  #[test]
  fn kak_hl_ranges_from_iter() {
    let source = "fn foo(a: i32, b: /* ® */ impl Into<Option<String>>) {}";
    let hl_names = vec![
      "constant",
      "function",
      "keyword",
      "variable",
      "punctuation",
      "type",
      "comment",
    ];

    let mut hl_conf = HighlightConfiguration::new(
      tree_sitter_rust::LANGUAGE.into(),
      "rust",
      tree_sitter_rust::HIGHLIGHTS_QUERY,
      tree_sitter_rust::INJECTIONS_QUERY,
      "",
    )
    .unwrap();
    hl_conf.configure(&hl_names);

    let mut hl = Highlighter::new();
    let events: Vec<_> = hl
      .highlight(&hl_conf, source.as_bytes(), None, |_| None)
      .unwrap()
      .flatten()
      .collect();

    assert_eq!(events.len(), 70);

    assert!(matches!(
      events[..],
      [
        HighlightEvent::HighlightStart(Highlight(2)),
        HighlightEvent::Source { start: 0, end: 2 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 2, end: 3 },
        HighlightEvent::HighlightStart(Highlight(1)),
        HighlightEvent::Source { start: 3, end: 6 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 6, end: 7 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(3)),
        HighlightEvent::Source { start: 7, end: 8 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 8, end: 9 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 9, end: 10 },
        HighlightEvent::HighlightStart(Highlight(5)),
        HighlightEvent::Source { start: 10, end: 13 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 13, end: 14 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 14, end: 15 },
        HighlightEvent::HighlightStart(Highlight(3)),
        HighlightEvent::Source { start: 15, end: 16 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 16, end: 17 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 17, end: 18 },
        HighlightEvent::HighlightStart(Highlight(6)),
        HighlightEvent::Source { start: 18, end: 26 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 26, end: 27 },
        HighlightEvent::HighlightStart(Highlight(2)),
        HighlightEvent::Source { start: 27, end: 31 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 31, end: 32 },
        HighlightEvent::HighlightStart(Highlight(5)),
        HighlightEvent::Source { start: 32, end: 36 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 36, end: 37 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(5)),
        HighlightEvent::Source { start: 37, end: 43 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 43, end: 44 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(5)),
        HighlightEvent::Source { start: 44, end: 50 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 50, end: 51 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 51, end: 52 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 52, end: 53 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::Source { start: 53, end: 54 },
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 54, end: 55 },
        HighlightEvent::HighlightEnd,
        HighlightEvent::HighlightStart(Highlight(4)),
        HighlightEvent::Source { start: 55, end: 56 },
        HighlightEvent::HighlightEnd
      ]
    ));
  }

  #[test]
  fn kak_hl_ranges_from_tree_house() {
    let source = RopeSlice::from("fn foo(a: i32, b: /* ® */ impl Into<Option<String>>) {}");
    let mut hl_names: Vec<_> = [
      "constant",
      "function",
      "keyword",
      "variable",
      "comment",
      "type",
      "punctuation",
      "punctuation.delimiter",
      "punctuation.bracket",
    ]
    .into_iter()
    .collect();

    // NOTE: sorting in descending order allows to ensure that we will always match against the longest (more accurate)
    // capture groups first; even though we have `punctuation`, parenthesis match `punctuation.bracket` and commas
    // match `punctuation.delimiter`, so we want to resolve them as the more accurate capture group for better
    // highlighting support.
    hl_names.sort_by_key(|&name| Reverse(name));

    let function_hl = hl_names
      .iter()
      .position(|&name| name == "function")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let keyword_hl = hl_names
      .iter()
      .position(|&name| name == "keyword")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let variable_hl = hl_names
      .iter()
      .position(|&name| name == "variable")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let comment_hl = hl_names
      .iter()
      .position(|&name| name == "comment")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let type_hl = hl_names
      .iter()
      .position(|&name| name == "type")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let punctuation_delimiter_hl = hl_names
      .iter()
      .position(|&name| name == "punctuation.delimiter")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();
    let punctuation_bracket_hl = hl_names
      .iter()
      .position(|&name| name == "punctuation.bracket")
      .map(|id| tree_house::highlighter::Highlight::new(id as _))
      .unwrap();

    let grammar: tree_house_bindings::Grammar = tree_sitter_rust::LANGUAGE.try_into().unwrap();
    let config = tree_house::LanguageConfig::new(
      grammar,
      tree_sitter_rust::HIGHLIGHTS_QUERY,
      tree_sitter_rust::INJECTIONS_QUERY,
      "",
    )
    .unwrap();

    config.configure(|name| {
      hl_names
        .iter()
        .position(|&name2| name.starts_with(name2))
        .map(|pos| tree_house::highlighter::Highlight::new(pos as _))
    });

    struct Loader {
      config: tree_house::LanguageConfig,
    }

    impl tree_house::LanguageLoader for Loader {
      fn language_for_marker(
        &self,
        _: tree_house::InjectionLanguageMarker,
      ) -> Option<tree_house::Language> {
        // NOTE: we won’t do injection so we can ignore this
        None
      }

      fn get_config(&self, _: tree_house::Language) -> Option<&tree_house::LanguageConfig> {
        // NOTE: same thing; we will only work in Rust
        Some(&self.config)
      }
    }

    let loader = Loader { config };
    let syntax = tree_house::Syntax::new(
      source,
      tree_house::Language::new(0),
      Duration::from_secs(1),
      &loader,
    )
    .unwrap();
    let mut highlighter = tree_house::highlighter::Highlighter::new(&syntax, source, &loader, ..);

    assert_eq!(highlighter.next_event_offset(), 0);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![keyword_hl]);

    assert_eq!(highlighter.next_event_offset(), 2);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 3);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![function_hl]);

    assert_eq!(highlighter.next_event_offset(), 6);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 7);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![variable_hl]);

    assert_eq!(highlighter.next_event_offset(), 8);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_delimiter_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 9);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 10);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![type_hl]);

    assert_eq!(highlighter.next_event_offset(), 13);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_delimiter_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 14);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 15);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![variable_hl]);

    assert_eq!(highlighter.next_event_offset(), 16);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_delimiter_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 17);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 18);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![comment_hl]);

    assert_eq!(highlighter.next_event_offset(), 26);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 27);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![keyword_hl]);

    assert_eq!(highlighter.next_event_offset(), 31);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 32);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![type_hl]);

    assert_eq!(highlighter.next_event_offset(), 36);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 37);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![type_hl]);

    assert_eq!(highlighter.next_event_offset(), 43);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 44);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![type_hl]);

    assert_eq!(highlighter.next_event_offset(), 50);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 51);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 52);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 53);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), 54);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Push);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 55);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(
      hls.into_iter().collect::<Vec<_>>(),
      vec![punctuation_bracket_hl]
    );

    assert_eq!(highlighter.next_event_offset(), 56);
    let (event, hls) = highlighter.advance();
    assert_eq!(event, tree_house::highlighter::HighlightEvent::Refresh);
    assert_eq!(hls.into_iter().collect::<Vec<_>>(), vec![]);

    assert_eq!(highlighter.next_event_offset(), u32::MAX);
  }
}
