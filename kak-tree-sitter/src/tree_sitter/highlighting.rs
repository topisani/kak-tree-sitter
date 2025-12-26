//! Convert from tree-sitter-highlight events to Kakoune ranges highlighter.

use std::fmt;

use ropey::RopeSlice;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineZeroIndexed(usize);

impl LineZeroIndexed {
  pub fn into_one_indexed(self) -> usize {
    self.0 + 1
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ColZeroIndexed(usize);

impl ColZeroIndexed {
  pub fn into_one_indexed(self) -> usize {
    self.0 + 1
  }
}

/// A convenient representation of a single highlight range for Kakoune.
///
/// `:doc highlighters`, `ranges`, for further documentation.
#[derive(Debug, Eq, PartialEq)]
pub struct KakHighlightRange {
  line_start: LineZeroIndexed,
  col_byte_start: ColZeroIndexed,
  line_end: LineZeroIndexed,
  col_byte_end: ColZeroIndexed,
  face: FaceId,
}

impl KakHighlightRange {
  fn new(
    line_start: LineZeroIndexed,
    col_byte_start: ColZeroIndexed,
    line_end: LineZeroIndexed,
    col_byte_end: ColZeroIndexed,
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
    source: RopeSlice,
    mut highlighter: tree_house::highlighter::Highlighter<'a, 'b, Languages>,
  ) -> Vec<Self> {
    let mut kak_hls = Vec::new();
    let mut tree_house_hls: Vec<tree_house::highlighter::Highlight> = Vec::new();

    // (byte, line, column)
    let mut last_position = (0, LineZeroIndexed(0), ColZeroIndexed(0));

    loop {
      let offset = highlighter.next_event_offset();
      if offset == u32::MAX {
        break;
      }

      let line = LineZeroIndexed(source.byte_to_line(offset as _));
      let col = ColZeroIndexed(offset as usize - source.line_to_byte(line.0));

      let (event, hl_list) = highlighter.advance();

      if !tree_house_hls.is_empty() {
        log::trace!("--> {tree_house_hls:?}");
        // for hl in &tree_house_hls {
        if let Some(hl) = tree_house_hls.last() {
          kak_hls.push(KakHighlightRange::new(
            last_position.1,
            last_position.2,
            line,
            col,
            FaceId::new(hl.idx()),
          ));
        }
      }

      match event {
        // apply highlights
        tree_house::highlighter::HighlightEvent::Refresh => {
          log::trace!("Refresh: {offset}, {line:?}, {col:?}");
          tree_house_hls.clear();
          tree_house_hls.extend(hl_list);
        }

        // stack up highlights
        tree_house::highlighter::HighlightEvent::Push => {
          tree_house_hls.extend(hl_list);
          log::trace!("Push: {offset}, {line:?}, {col:?}; {tree_house_hls:?}");
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
      self.line_start.into_one_indexed(),
      self.col_byte_start.into_one_indexed(),
      self.line_end.into_one_indexed(),
      self.col_byte_end.0.max(1), // upper bound is inclusive and 0 is not allowed
      faces[self.face.id]
    )
  }
}

#[cfg(test)]
mod tests {
  use std::{cmp::Reverse, time::Duration};

  use ropey::RopeSlice;

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
