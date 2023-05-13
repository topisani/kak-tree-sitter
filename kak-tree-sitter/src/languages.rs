//! Supported languages.
//!
//! Languages have different objects (grammars, queries, etc.) living at runtime and must be loaded beforehand.

use std::{collections::HashMap, path::Path};

use kak_tree_sitter_config::Config;
use libloading::Symbol;
use tree_sitter_highlight::HighlightConfiguration;

use crate::queries::Queries;

pub struct Language {
  // NOTE: we need to keep that alive *probably*; better be safe than sorry
  #[allow(dead_code)]
  pub ts_lib: libloading::Library,

  pub ts_lang: tree_sitter::Language,

  pub queries: Queries,

  pub hl_config: HighlightConfiguration,
}

pub struct Languages {
  /// Map a `filetype` to the tree-sitter [`Language`] and its queries.
  langs: HashMap<String, Language>,
}

impl Languages {
  /// Load a grammar.
  fn load_grammar(lang: &str, path: &Path) -> Option<(libloading::Library, tree_sitter::Language)> {
    let lib = unsafe { libloading::Library::new(path) };
    match lib {
      Ok(lib) => {
        let fn_sym = format!("tree_sitter_{}", lang);

        let sym: Result<Symbol<fn() -> tree_sitter::Language>, _> =
          unsafe { lib.get(fn_sym.as_bytes()) };
        match sym {
          Ok(sym) => {
            let ffi_lang = sym();
            Some((lib, ffi_lang))
          }

          Err(err) => {
            eprintln!("cannot find {lang}: {err}");
            None
          }
        }
      }

      Err(err) => {
        eprintln!("cannot load grammar {}: {err}", path.display());
        None
      }
    }
  }

  /// Load languages.
  ///
  /// This function will scan the directory and extract / map all the languages.
  pub fn load_from_dir(config: &Config) -> Self {
    let mut langs = HashMap::new();

    // iterate over all known languages in the configuration
    for lang_name in config.languages.language.keys() {
      if let Some(grammar_path) = config.languages.get_grammar_path(lang_name) {
        if let Some((ts_lib, ts_lang)) = Self::load_grammar(lang_name, &grammar_path) {
          if let Some(queries_dir) = config.languages.get_queries_dir(lang_name) {
            let queries = Queries::load_from_dir(queries_dir);
            let mut hl_config = HighlightConfiguration::new(
              ts_lang.clone(),
              queries.highlights.as_deref().unwrap_or(""),
              queries.injections.as_deref().unwrap_or(""),
              queries.locals.as_deref().unwrap_or(""),
            )
            .unwrap();
            hl_config.configure(&config.highlight.hl_names);

            let lang = Language {
              ts_lang,
              ts_lib,
              queries,
              hl_config,
            };
            langs.insert(lang_name.to_owned(), lang);
          }
        }
      }
    }

    Self { langs }
  }

  pub fn get(&self, filetype: impl AsRef<str>) -> Option<&Language> {
    self.langs.get(filetype.as_ref())
  }
}