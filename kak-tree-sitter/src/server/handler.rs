use std::{
  collections::HashMap,
  sync::atomic::{AtomicUsize, Ordering},
};

use kak_tree_sitter_config::Config;
use mio::Token;

use crate::{
  error::OhNo,
  kakoune::{buffer::BufferId, selection::Sel, text_objects::OperationMode},
  protocol::response::{EnabledLang, Payload, Response},
  tree_sitter::{
    languages::{CachedLanguage, Languages},
    nav,
    state::Trees,
  },
};

use super::resources::ServerResources;

/// Type responsible for handling tree-sitter requests.
///
/// This type is stateful, as requests might have side-effect (i.e. tree-sitter
/// parsing generates trees/highlighters that can be reused, for instance).
pub struct Handler {
  config: Config,
  trees: Trees,
  langs: Languages,
  with_highlighting: bool,
}

impl Handler {
  pub fn new(config: &Config, with_highlighting: bool) -> Self {
    Self {
      config: config.clone(),
      trees: Trees::default(),
      langs: Languages::new(),
      with_highlighting,
    }
  }

  /// Initiate languages on session init.
  pub fn handle_session_begin(&mut self) -> Payload {
    let enabled_langs = self
      .config
      .languages
      .iter()
      .map(|(name, lang)| EnabledLang {
        name: name.to_owned(),
        remove_default_highlighter: lang.remove_default_highlighter.into(),
        filetype_hook: lang.filetype_hook.into(),
        aliases: lang.aliases.clone(),
      })
      .collect();
    Payload::Init { enabled_langs }
  }

  /// Update buffer metadata change.
  pub fn handle_buffer_metadata(
    &mut self,
    resources: &mut ServerResources,
    id: &BufferId,
    lang: &str,
  ) -> Result<Payload, OhNo> {
    let lang = self.langs.get_or_load(&self.config, lang)?;
    let tree = self.trees.compute(resources, lang, id)?;
    let fifo = tree.fifo();
    let fifo_path = fifo.path().to_owned();
    let sentinel = fifo.sentinel().to_owned();

    Ok(Payload::BufferSetup {
      fifo_path,
      sentinel,
    })
  }

  /// Handle buffer close.
  pub fn handle_buffer_close(&mut self, id: &BufferId) {
    self.trees.delete_tree(id);
  }

  /// Update a full buffer update.
  pub fn handle_full_buffer_update(&mut self, tkn: Token) -> Result<Option<Response>, OhNo> {
    let id = self.trees.get_buf_id(&tkn)?.clone();
    log::debug!("updating {id:?}, token {tkn:?}");
    let tree = self.trees.get_tree_mut(&id)?;

    // update the tree
    if !tree.update_buf()? {
      // early return if no update occurred
      return Ok(None);
    }

    // run any additional post-processing on the buffer
    if !self.with_highlighting {
      return Ok(None);
    }

    let ranges = loop {
      // NOTE: this pattern is disgusting, but we cannot use get_or_load() here, because we do not want to
      // have a mutable borrow on self.langs as we inject languages
      let lang = match self.langs.get(tree.lang()) {
        Ok(lang) => lang,

        Err(OhNo::TriedLoadingOnceLang { lang }) => {
          log::debug!("higlight aborted for {lang}; loading failed in the past");
          return Ok(None);
        }

        Err(_) => {
          let lang = match Languages::load_lang(&self.config, tree.lang()) {
            Ok(lang) => lang,
            Err(err) => {
              self
                .langs
                .insert_lang(tree.lang(), CachedLanguage::LoadFailed);
              return Err(err);
            }
          };
          self.langs.insert_lang(tree.lang(), lang);
          self.langs.get(tree.lang())?
        }
      };

      let mut was_cancelled_langs: HashMap<String, CachedLanguage> = HashMap::new();
      let cancellation = AtomicUsize::new(0);

      let ranges = tree
        .highlight(lang, Some(&cancellation), |inject_lang| {
          match self.langs.get(inject_lang) {
            // injected language is already loaded; we can continue with it
            Ok(lang2) => Some(&lang2.hl_config),

            // that language won’t be loaded anymore; no injection
            Err(OhNo::TriedLoadingOnceLang { .. }) => None,

            // injected language is not loaded / doesn’t exist
            Err(_) => {
              if !was_cancelled_langs
                .iter()
                .any(|(lang_name, _)| lang_name.as_str() == inject_lang)
              {
                // load the injected language; if it fails, just ensure we won’t try again
                let lang2 = Languages::load_lang(&self.config, inject_lang)
                  .map_or(CachedLanguage::LoadFailed, CachedLanguage::from);

                log::debug!(
                  "{inject_lang} loaded causing {lang_name} highlight to restart",
                  lang_name = lang.lang_name()
                );
                was_cancelled_langs.insert(inject_lang.to_owned(), lang2);

                // we cannot continue highlighting, so abort and will re-highlight again
                cancellation.store(1, Ordering::SeqCst);
              }

              None
            }
          }
        })
        .map_or_else(
          // if we were cancelled, this is not a hard error; we will retry again with the injection language
          |err| match err {
            OhNo::HighlightError {
              err: tree_sitter_highlight::Error::Cancelled,
            } => Ok(Vec::new()),

            // anything else is worth stopping
            _ => Err(err),
          },
          Ok,
        )?;

      // we were cancelled due to loaded languages; inject to the regular map
      let was_not_cancelled = was_cancelled_langs.is_empty();
      for (lang_name, lang) in was_cancelled_langs {
        self.langs.insert_lang(lang_name, lang);
      }

      if was_not_cancelled {
        break ranges;
      }
    };

    let resp = Response::new(
      id.session(),
      None,
      id.buffer().to_owned(),
      Payload::Highlights { ranges },
    );

    Ok(Some(resp))
  }

  pub fn handle_text_objects(
    &mut self,
    id: &BufferId,
    pattern: &str,
    selections: &[Sel],
    mode: &OperationMode,
  ) -> Result<Payload, OhNo> {
    log::debug!("text-objects {pattern} for buffer {id:?}");

    let tree_state = self.trees.get_tree(id)?;
    let lang = self.langs.get_or_load(&self.config, tree_state.lang())?;
    let sels = tree_state.text_objects(lang, pattern, selections, mode)?;

    Ok(Payload::Selections { sels })
  }

  pub fn handle_nav(
    &mut self,
    id: &BufferId,
    selections: &[Sel],
    dir: nav::Dir,
  ) -> Result<Payload, OhNo> {
    log::debug!("nav {dir:?} for buffer {id:?}");

    let tree_state = self.trees.get_tree(id)?;
    let sels = tree_state.nav_tree(selections, dir);

    Ok(Payload::Selections { sels })
  }
}
