//! Supported languages.
//!
//! Languages have different objects (grammars, queries, etc.) living at runtime and must be loaded beforehand.

use std::collections::{HashMap, hash_map::Entry};

use kak_tree_sitter_config::{Config, LanguageConfig, LanguagesConfig};
use libloading::Symbol;
use tree_sitter::Query;
use tree_sitter_highlight::HighlightConfiguration;

use crate::{error::OhNo, tree_sitter::queries::Queries};

pub struct Language {
  pub name: String,
  pub hl_config: HighlightConfiguration,
  pub hl_names: Vec<String>,
  // query to use for text objects, if supported by the language
  pub textobject_query: Option<Query>,

  // NOTE: we need to keep that alive *probably*; better be safe than sorry
  ts_lang: tree_sitter::Language,
  _ts_lib: libloading::Library,
}

impl Language {
  pub fn lang_name(&self) -> &str {
    &self.name
  }

  pub fn lang(&self) -> &tree_sitter::Language {
    &self.ts_lang
  }
}

/// A cached language, or blocklisted one.
///
/// A blocklisted language been tried to get loaded once, but we were not
/// able to load, so we still inject the language to prevent trying again
/// later.
pub enum CachedLanguage {
  Loaded(Box<Language>),
  LoadFailed,
}

impl From<Language> for CachedLanguage {
  fn from(lang: Language) -> Self {
    Self::Loaded(Box::new(lang))
  }
}

/// All loaded languages that can be used to parse buffers.
///
/// For a language to be tree-sitter compatible, it has to have a mapping in
/// the underlying map. If not, the language might be loaded on-demand.
pub struct Languages {
  /// Map a `kts_lang` to the tree-sitter [`Language`] and its queries.
  langs: HashMap<String, CachedLanguage>,
}

impl Languages {
  pub fn new() -> Self {
    Self {
      langs: HashMap::new(),
    }
  }

  fn load_grammar(
    lang_name: &str,
    lang_config: &LanguageConfig,
  ) -> Result<(libloading::Library, tree_sitter::Language), OhNo> {
    let Some(path) = LanguagesConfig::get_grammar_path(lang_config, lang_name) else {
      return Err(OhNo::CannotLoadGrammar {
        lang: lang_name.to_owned(),
        err: format!("no grammar path for language {lang_name}"),
      });
    };
    log::debug!("  grammar path: {}", path.display());

    let lib = unsafe { libloading::Library::new(path) };
    let lib = lib.map_err(|err| OhNo::CannotLoadGrammar {
      lang: lang_name.to_owned(),
      err: err.to_string(),
    })?;
    let fn_sym = format!("tree_sitter_{}", lang_name.replace(['.', '-'], "_"));

    let sym: Result<Symbol<fn() -> tree_sitter::Language>, _> =
      unsafe { lib.get(fn_sym.as_bytes()) };
    let sym = sym.map_err(|err| OhNo::CannotLoadGrammar {
      lang: lang_name.to_owned(),
      err: format!("cannot find language: {err}"),
    })?;
    let sym = sym();

    Ok((lib, sym))
  }

  fn load_queries(lang_name: &str, lang_config: &LanguageConfig) -> Result<Queries, OhNo> {
    let Some(queries_dir) = LanguagesConfig::get_queries_dir(lang_config, lang_name) else {
      return Err(OhNo::CannotLoadQueries {
        lang: lang_name.to_owned(),
        err: format!("no queries for language {lang_name}"),
      });
    };
    log::debug!("  queries directory: {}", queries_dir.display());

    Ok(Queries::load_from_dir(queries_dir))
  }

  /// Load a specific language.
  pub fn load_lang(config: &Config, lang_name: impl AsRef<str>) -> Result<Language, OhNo> {
    let lang_name = lang_name.as_ref();
    log::info!("loading configuration for {lang_name}");

    let lang_config = config.languages.get_lang_config(lang_name)?;
    let (ts_lib, ts_lang) = Self::load_grammar(lang_name, lang_config)?;
    let queries = Self::load_queries(lang_name, lang_config)?;

    let mut hl_config = HighlightConfiguration::new(
      ts_lang.clone(),
      lang_name,
      queries.highlights.as_deref().unwrap_or(""),
      queries.injections.as_deref().unwrap_or(""),
      queries.locals.as_deref().unwrap_or(""),
    )?;
    let hl_names: Vec<_> = config.highlight.groups.iter().cloned().collect();
    hl_config.configure(&hl_names);

    let textobject_query = queries
      .text_objects
      .as_deref()
      .map(|q| Query::new(&ts_lang, q).map(Some))
      .unwrap_or_else(|| Ok(None))?;

    let lang = Language {
      name: lang_name.to_owned(),
      hl_config,
      hl_names,
      textobject_query,
      ts_lang,
      _ts_lib: ts_lib,
    };

    Ok(lang)
  }

  /// Return a [`Language`] if exists.
  pub fn get(&self, lang: impl AsRef<str>) -> Result<&Language, OhNo> {
    let lang_name = lang.as_ref();
    self
      .langs
      .get(lang_name)
      .ok_or_else(|| OhNo::UnknownLang {
        lang: lang_name.to_owned(),
      })
      .and_then(|lang| match lang {
        CachedLanguage::Loaded(language) => Ok(language.as_ref()),
        CachedLanguage::LoadFailed => Err(OhNo::TriedLoadingOnceLang {
          lang: lang_name.to_owned(),
        }),
      })
  }

  /// Manually inject a language.
  pub fn insert_lang(&mut self, lang_name: impl Into<String>, lang: impl Into<CachedLanguage>) {
    self.langs.insert(lang_name.into(), lang.into());
  }

  /// Get the [`Language`] associated with the argument name, or try to load it.
  pub fn get_or_load(
    &mut self,
    config: &Config,
    lang_name: impl AsRef<str>,
  ) -> Result<&Language, OhNo> {
    let lang_name = lang_name.as_ref();

    match self.langs.entry(lang_name.to_owned()) {
      Entry::Occupied(lang) => match lang.into_mut() {
        CachedLanguage::Loaded(language) => Ok(language),
        CachedLanguage::LoadFailed => Err(OhNo::TriedLoadingOnceLang {
          lang: lang_name.to_owned(),
        }),
      },

      Entry::Vacant(entry) => match entry.insert(Self::load_lang(config, lang_name)?.into()) {
        CachedLanguage::Loaded(language) => Ok(language),
        CachedLanguage::LoadFailed => unreachable!(),
      },
    }
  }
}
