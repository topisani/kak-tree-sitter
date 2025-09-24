//! Tree-sitter state (i.e. highlighting, tree walking, etc.)

use std::{
  collections::{HashMap, hash_map::Entry},
  time::Duration,
};

use ropey::RopeSlice;
use tree_house::text_object::CapturedNode;
use tree_house_bindings::{InactiveQueryCursor, Node};

use crate::{
  error::OhNo,
  kakoune::{
    buffer::BufferId,
    selection::{ObjectFlags, Pos, Sel, SelectMode},
    text_objects::OperationMode,
  },
  server::triple_buffer::TripleBufferReader,
  tree_sitter::languages::Languages,
};

use super::{highlighting::KakHighlightRange, languages::Language, nav};

/// Lang-keyed trees.
#[derive(Default)]
pub struct Trees {
  trees: HashMap<BufferId, TreeState>,
}

impl Trees {
  /// Create or update a tree.
  pub fn compute(
    &mut self,
    languages: &Languages,
    lang: &Language,
    id: &BufferId,
  ) -> Result<&mut TreeState, OhNo> {
    match self.trees.entry(id.clone()) {
      Entry::Occupied(entry) => {
        let tree = entry.into_mut();
        tree.change_lang(languages, lang)?;
        Ok(tree)
      }

      Entry::Vacant(entry) => {
        let tree = TreeState::new(languages, lang)?;
        Ok(entry.insert(tree))
      }
    }
  }

  pub fn get_tree(&self, id: &BufferId) -> Result<&TreeState, OhNo> {
    self
      .trees
      .get(id)
      .ok_or_else(|| OhNo::UnknownBuffer { id: id.clone() })
  }

  pub fn get_tree_mut(&mut self, id: &BufferId) -> Result<&mut TreeState, OhNo> {
    self
      .trees
      .get_mut(id)
      .ok_or_else(|| OhNo::UnknownBuffer { id: id.clone() })
  }

  pub fn delete_tree(&mut self, id: &BufferId) {
    self.trees.remove(id);
  }

  pub fn clean_session(&mut self, session: &str) {
    self.trees.retain(|id, _| id.session() != session);
  }
}

/// State around a tree.
///
/// A tree-sitter tree represents a parsed buffer in a given state. It can be walked with queries and updated.
pub struct TreeState {
  buf: String,
  lang_name: String,
  lang: tree_house::Language,
  syntax: tree_house::Syntax,
}

impl TreeState {
  pub fn new(languages: &Languages, lang: &Language) -> Result<Self, OhNo> {
    let syntax = tree_house::Syntax::new(
      RopeSlice::from(""),
      lang.language(),
      Duration::from_millis(100),
      languages,
    )
    .map_err(|err| OhNo::TreeHouse {
      err: err.to_string(),
    })?;

    Ok(Self {
      buf: String::default(),
      lang_name: lang.name.clone(),
      lang: lang.language(),
      syntax,
    })
  }

  pub fn lang_name(&self) -> &str {
    &self.lang_name
  }

  pub fn change_lang(&mut self, languages: &Languages, lang: &Language) -> Result<(), OhNo> {
    lang.lang_name().clone_into(&mut self.lang_name);
    self.recompute_tree(languages)
  }

  /// Read a triple buffer to replace our internal buffer.
  ///
  /// Return `true` if the buffer has changed.
  pub fn update_buf(
    &mut self,
    languages: &Languages,
    reader: TripleBufferReader,
  ) -> Result<bool, OhNo> {
    let changed = reader.read_to(&mut self.buf);

    if changed {
      self.recompute_tree(languages)?;
    }

    Ok(changed)
  }

  pub fn recompute_tree(&mut self, languages: &Languages) -> Result<(), OhNo> {
    self.syntax = tree_house::Syntax::new(
      RopeSlice::from(self.buf.as_str()),
      self.lang,
      Duration::from_millis(100),
      languages,
    )
    .map_err(|err| OhNo::TreeHouse {
      err: err.to_string(),
    })?;

    Ok(())
  }

  pub fn highlight(&mut self, langs: &Languages) -> Result<Vec<KakHighlightRange>, OhNo> {
    let highlighter = tree_house::highlighter::Highlighter::new(
      &self.syntax,
      RopeSlice::from(self.buf.as_str()),
      langs,
      ..,
    );

    let hls = KakHighlightRange::from_tree_house(&self.buf, highlighter);

    Ok(hls)
  }

  /// Get the text-objects for the given pattern.
  ///
  /// This function takes in a list of selections and a mode of operation, and return new selections, depending on the
  /// mode.
  pub fn text_objects(
    &self,
    lang: &Language,
    pattern: &str,
    selections: &[Sel],
    mode: &OperationMode,
  ) -> Result<Vec<Sel>, OhNo> {
    let query = lang
      .textobject_query
      .as_ref()
      .ok_or(OhNo::UnsupportedTextObjects)?;

    let buf = RopeSlice::from(self.buf.as_str());

    // get captures’ nodes for the given pattern; this is a function because the pattern might be dynamically recomputed
    // (e.g. object mode)
    let get_captures_nodes = |pattern| {
      let nodes = query
        .capture_nodes(
          pattern,
          self.syntax.tree().root_node(),
          buf,
          InactiveQueryCursor::default(),
        )
        .map(|iter| {
          iter.map(|captured| match captured {
            CapturedNode::Single(node) => node,

            // SAFETY: `nodes` is guaranteed to always have at least one node
            CapturedNode::Grouped(nodes) => nodes.into_iter().next().unwrap(),
          })
        })
        .ok_or(OhNo::UnknownTextObjectQuery {
          pattern: pattern.to_owned(),
        })?;

      <Result<_, OhNo>>::Ok(nodes)
    };

    let sels = match mode {
      OperationMode::SearchNext => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_search_next_text_object(buf, sel, &mut nodes))
          .collect()
      }

      OperationMode::SearchPrev => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_search_prev_text_object(buf, sel, &mut nodes))
          .collect()
      }

      OperationMode::SearchExtendNext => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_search_extend_next_text_object(buf, sel, &mut nodes))
          .collect()
      }

      OperationMode::SearchExtendPrev => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_search_extend_prev_text_object(buf, sel, &mut nodes))
          .collect()
      }

      OperationMode::FindNext => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_find_text_object(buf, sel, &mut nodes, false))
          .collect()
      }

      OperationMode::FindPrev => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_find_text_object(buf, sel, &mut nodes, true))
          .collect()
      }

      OperationMode::ExtendNext => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_extend_text_object(buf, sel, &mut nodes, false))
          .collect()
      }

      OperationMode::ExtendPrev => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_extend_text_object(buf, sel, &mut nodes, true))
          .collect()
      }

      OperationMode::Select => {
        let mut nodes = get_captures_nodes(pattern)?;
        selections
          .iter()
          .flat_map(|sel| {
            Self::tree_sitter_select_text_object(buf, sel, &mut nodes).collect::<Vec<_>>()
          })
          .collect()
      }

      OperationMode::Object { mode, flags } => {
        let flags = ObjectFlags::parse_kak_str(flags);

        let pattern = format!(
          "{pattern}.{}",
          if flags.inner { "inside" } else { "around" }
        );
        let mut nodes = get_captures_nodes(&pattern)?;

        selections
          .iter()
          .flat_map(|sel| Self::tree_sitter_object_text_object(buf, sel, &mut nodes, *mode, flags))
          .collect()
      }
    };

    Ok(sels)
  }

  /// Search the next text-object for a given selection.
  fn tree_sitter_search_next_text_object<'a>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Sel> {
    let p = sel.anchor.max(sel.cursor);
    let node = Self::tree_sitter_node_after(buf, &p, nodes)?;
    let start = Pos::from_tree_sitter(buf, node.start_byte());
    let mut end = Pos::from_tree_sitter(buf, node.end_byte());
    end.col -= 1;

    Some(sel.replace(&start, &end))
  }

  /// Search the prev text-object for a given selection.
  fn tree_sitter_search_prev_text_object<'a>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Sel> {
    let p = sel.anchor.min(sel.cursor);
    let node = Self::tree_sitter_node_before(buf, &p, nodes)?;
    let start = Pos::from_tree_sitter(buf, node.start_byte());
    let mut end = Pos::from_tree_sitter(buf, node.end_byte());
    end.col -= 1;

    Some(sel.replace(&start, &end))
  }

  /// Search-extend the next text-object for a given selection.
  fn tree_sitter_search_extend_next_text_object<'a>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Sel> {
    let node = Self::tree_sitter_node_after(buf, &sel.cursor, nodes)?;
    let cursor = Pos::from_tree_sitter(buf, node.start_byte());

    Some(Sel {
      anchor: sel.anchor,
      cursor,
    })
  }

  /// Search extend the prev text-object for a given selection.
  fn tree_sitter_search_extend_prev_text_object<'a>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Sel> {
    let node = Self::tree_sitter_node_before(buf, &sel.cursor, nodes)?;
    let cursor = Pos::from_tree_sitter(buf, node.start_byte());

    Some(Sel {
      anchor: sel.anchor,
      cursor,
    })
  }

  /// Find the next/prev text-object for a given selection.
  fn tree_sitter_find_text_object<'a>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'a>>,
    is_prev: bool,
  ) -> Option<Sel> {
    let node = if is_prev {
      Self::tree_sitter_node_before(buf, &sel.cursor, nodes)?
    } else {
      Self::tree_sitter_node_after(buf, &sel.cursor, nodes)?
    };
    let cursor = Pos::from_tree_sitter(buf, node.start_byte());
    let anchor = sel.cursor;

    Some(Sel { anchor, cursor })
  }

  /// Extend onto the next/prev text-object for a given selection.
  fn tree_sitter_extend_text_object<'node>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'node>>,
    is_prev: bool,
  ) -> Option<Sel> {
    let node = if is_prev {
      Self::tree_sitter_node_before(buf, &sel.cursor, nodes)?
    } else {
      Self::tree_sitter_node_after(buf, &sel.cursor, nodes)?
    };
    let cursor = Pos::from_tree_sitter(buf, node.start_byte());
    let anchor = sel.anchor;

    Some(Sel { anchor, cursor })
  }

  /// Select text-object occurrences inside the current selection.
  fn tree_sitter_select_text_object<'node>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'node>>,
  ) -> impl Iterator<Item = Sel> {
    nodes
      .filter(move |node| sel.selects(buf, node))
      .map(move |node| {
        let start = Pos::from_tree_sitter(buf, node.start_byte());
        let end = Pos::from_tree_sitter(buf, node.end_byte());
        sel.replace(&start, &end)
      })
  }

  /// Object-mode text-objects.
  ///
  /// Object-mode is a special in Kakoune aggregating many features, allowing to match inner / whole objects. The
  /// tree-sitter version enhances the mode with all possible tree-sitter capture groups.
  fn tree_sitter_object_text_object<'node>(
    buf: RopeSlice,
    sel: &Sel,
    nodes: impl Iterator<Item = Node<'node>>,
    mode: SelectMode,
    flags: ObjectFlags,
  ) -> Option<Sel> {
    let node = Self::tree_sitter_narrowest_enclosing_node(buf, &sel.cursor, nodes)?;

    match mode {
      // extend only moves the cursor
      SelectMode::Extend => {
        let anchor = sel.anchor;
        let cursor = if flags.to_begin {
          Pos::from_tree_sitter(buf, node.start_byte())
        } else if flags.to_end {
          let mut p = Pos::from_tree_sitter(buf, node.end_byte());
          p.col -= 1;
          p
        } else {
          return None;
        };

        Some(Sel { anchor, cursor })
      }

      SelectMode::Replace => {
        // brute force but eh it works lol
        if flags.to_begin && !flags.to_end {
          let anchor = sel.cursor;
          let cursor = Pos::from_tree_sitter(buf, node.start_byte());
          Some(Sel { anchor, cursor })
        } else if !flags.to_begin && flags.to_end {
          let anchor = sel.cursor;
          let mut cursor = Pos::from_tree_sitter(buf, node.end_byte());
          cursor.col -= 1;
          Some(Sel { anchor, cursor })
        } else if flags.to_begin && flags.to_end {
          let anchor = Pos::from_tree_sitter(buf, node.start_byte());
          let mut cursor = Pos::from_tree_sitter(buf, node.end_byte());
          cursor.col -= 1;
          Some(Sel { anchor, cursor })
        } else {
          None
        }
      }
    }
  }

  /// Get the next node after given position.
  fn tree_sitter_node_after<'a>(
    buf: RopeSlice,
    p: &Pos,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Node<'a>> {
    let mut candidates = nodes
      .filter(|node| &Pos::from_tree_sitter(buf, node.start_byte()) > p)
      .collect::<Vec<_>>();

    candidates.sort_by_key(|node| node.start_byte());
    candidates.into_iter().next()
  }

  /// Get the previous node before a given position.
  fn tree_sitter_node_before<'a>(
    buf: RopeSlice,
    p: &Pos,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Node<'a>> {
    let mut candidates = nodes
      .filter(|node| &Pos::from_tree_sitter(buf, node.start_byte()) < p)
      .collect::<Vec<_>>();

    candidates.sort_by_key(|node| node.start_byte());
    candidates.into_iter().next_back()
  }

  /// Get the narrowest enclosing node of a given position.
  fn tree_sitter_narrowest_enclosing_node<'a>(
    buf: RopeSlice,
    p: &Pos,
    nodes: impl Iterator<Item = Node<'a>>,
  ) -> Option<Node<'a>> {
    let mut candidates = nodes
      .filter(|node| {
        &Pos::from_tree_sitter(buf, node.start_byte()) < p
          && &Pos::from_tree_sitter(buf, node.end_byte()) > p
      })
      .collect::<Vec<_>>();

    candidates.sort_by_key(|node| node.start_byte());
    candidates.into_iter().next_back()
  }

  /// Navigate the tree.
  ///
  /// This function will apply the direction on all selections, expanding or collapsing them. If a selection is not
  /// spanning on a node, the closet node is selected first, so that if you have the cursor and anchor at the same
  /// location and you want to select the next child, your cursor will expand to the whole nearest enclosing node first.
  pub fn tree_sitter_nav_tree(&self, selections: &[Sel], dir: nav::Dir) -> Vec<Sel> {
    let buf = RopeSlice::from(self.buf.as_str());

    selections
      .iter()
      .map(|sel| {
        self
          .tree_sitter_find_sel_node(buf, sel)
          .and_then(|node| {
            // if our selection is not the same as the node, we pick the node
            if !sel.fully_selects(buf, &node) {
              log::debug!("selection {sel:?} doesn’t fully select node {node:?}");
              return Some(node);
            }

            log::debug!("walking node {node:?} for dir {dir:?}");
            log::debug!("  parent: {:?}", node.parent());
            log::debug!("  1st child: {:?}", node.child(0));
            log::debug!("  next sibling: {:?}", node.next_sibling());

            let res = match dir {
              nav::Dir::Parent => node.parent(),
              nav::Dir::FirstChild => node.child(0),
              nav::Dir::LastChild => node
                .child_count()
                .checked_sub(1)
                .and_then(|i| node.child(i)),
              nav::Dir::FirstSibling => node.parent().and_then(|parent| parent.child(0)),
              nav::Dir::LastSibling => node.parent().and_then(|parent| {
                parent
                  .child_count()
                  .checked_sub(1)
                  .and_then(|i| parent.child(i))
              }),
              nav::Dir::PrevSibling { cousin } if cousin => {
                Self::tree_sitter_find_prev_sibling_or_cousin(&node)
              }
              nav::Dir::NextSibling { cousin } if cousin => {
                Self::tree_sitter_find_next_sibling_or_cousin(&node)
              }
              nav::Dir::PrevSibling { .. } => node.prev_sibling(),
              nav::Dir::NextSibling { .. } => node.next_sibling(),
            };

            log::debug!("navigated to node: {res:?}");
            res
          })
          .map(|node| sel.replace_with_node(buf, &node))
          .unwrap_or_else(|| sel.clone())
      })
      .collect()
  }

  /// Find the node for a selection.
  fn tree_sitter_find_sel_node(&self, buf: RopeSlice, sel: &Sel) -> Option<Node<'_>> {
    log::trace!("finding node for selection {sel:?}");

    let start = sel.anchor.min(sel.cursor);
    let mut end = sel.cursor.max(sel.anchor);
    end.col += 1; // Kakoune ranges are inclusive
    let node = self
      .syntax
      .tree()
      .root_node()
      // FIXME: we need bytes here…
      .descendant_for_byte_range(start.into_tree_sitter(buf) as _, end.into_tree_sitter(buf) as _);

    log::trace!("found node: {node:?}");

    node
  }

  /// Get the next sibiling or cousin.
  fn tree_sitter_find_next_sibling_or_cousin<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    node.next_sibling().or_else(|| {
      let parent = node.parent()?;
      let parent_sibling = parent.next_sibling()?;

      if parent_sibling.child_count() > 0 {
        parent_sibling.child(0)
      } else {
        None
      }
    })
  }

  /// Get the previous sibiling or cousin.
  fn tree_sitter_find_prev_sibling_or_cousin<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    node.prev_sibling().or_else(|| {
      let parent = node.parent()?;
      let parent_sibling = parent.prev_sibling()?;

      if parent_sibling.child_count() > 0 {
        parent_sibling.child(parent_sibling.child_count() - 1)
      } else {
        None
      }
    })
  }
}
