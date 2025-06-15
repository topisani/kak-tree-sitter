//! Supported languages.
//!
//! Languages have different objects (grammars, queries, etc.) living at runtime and must be loaded beforehand.

use std::{
  cell::{LazyCell, RefCell},
  collections::HashMap,
  ops::Deref,
  rc::Rc,
};

use kak_tree_sitter_config::{
  Config, GrammarConfig, GrammarsConfig, LanguageConfig, LanguagesConfig,
};
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

  grammar: Rc<Grammar>,
}

impl Language {
  pub fn lang_name(&self) -> &str {
    &self.name
  }

  pub fn grammar(&self) -> &Rc<Grammar> {
    &self.grammar
  }
}

pub struct Grammar {
  ts_lang: tree_sitter::Language,
  _ts_lib: libloading::Library,
}

impl Grammar {
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
///
/// [`LazyCell`] is used to load the language only when first accessed.
pub struct Languages {
  /// Map a `kts_lang` to the tree-sitter [`Language`] and its queries.
  langs: HashMap<String, LazyLang>,
}

type LazyLang = LazyCell<CachedLanguage, Box<dyn FnOnce() -> CachedLanguage + 'static>>;
pub type GrammarCache = Rc<RefCell<HashMap<String, Rc<Grammar>>>>;

impl Languages {
  pub fn new(config: &Config) -> Self {
    let grammars: Rc<RefCell<HashMap<String, Rc<Grammar>>>> = Rc::new(RefCell::new(HashMap::new()));

    let langs = config
      .languages
      .iter()
      .map(|(lang_name, _)| {
        let config = config.clone();
        let lang_name2 = lang_name.to_owned();
        let grammars = grammars.clone();

        let lazy = LazyLang::new(Box::new(move || {
          match Self::load_lang(&grammars, &config, &lang_name2) {
            Ok(lang) => CachedLanguage::Loaded(Box::new(lang)),

            Err(err) => {
              log::error!("cannot lazy load language '{lang_name2}'; will not try again: {err}");
              CachedLanguage::LoadFailed
            }
          }
        }));

        (lang_name.to_owned(), lazy)
      })
      .collect();
    Self { langs }
  }

  fn load_grammar(lang_name: &str, grammar_config: &GrammarConfig) -> Result<Grammar, OhNo> {
    let Some(path) = GrammarsConfig::get_grammar_path(grammar_config, lang_name) else {
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
    let ts_lang = sym();

    Ok(Grammar {
      ts_lang,
      _ts_lib: lib,
    })
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
  pub fn load_lang(
    grammars: &GrammarCache,
    config: &Config,
    lang_name: impl AsRef<str>,
  ) -> Result<Language, OhNo> {
    let lang_name = lang_name.as_ref();
    log::info!("loading configuration for {lang_name}");

    let lang_config = config.languages.get_lang_config(lang_name)?;
    let grammar_name = lang_config.grammar.as_deref().unwrap_or(lang_name);
    let grammar_config = config.grammars.get_grammar_config(grammar_name)?;

    // load the grammar if not already cached
    let grammar = if let Some(grammar) = grammars.borrow().get(lang_name) {
      log::debug!("grammar {lang_name} alread loaded; using cached version");
      grammar.clone()
    } else {
      let grammar = Self::load_grammar(grammar_name, grammar_config)?;

      let grammar = Rc::new(grammar);
      grammars
        .borrow_mut()
        .insert(lang_name.to_owned(), grammar.clone());

      grammar
    };

    let queries = Self::load_queries(lang_name, lang_config)?;

    let mut hl_config = HighlightConfiguration::new(
      grammar.lang().clone(),
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
      .map(|q| Query::new(grammar.lang(), q).map(Some))
      .unwrap_or_else(|| Ok(None))?;

    let lang = Language {
      name: lang_name.to_owned(),
      hl_config,
      hl_names,
      textobject_query,
      grammar,
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
      .and_then(|lang| match lang.deref() {
        CachedLanguage::Loaded(language) => Ok(language.as_ref()),
        CachedLanguage::LoadFailed => Err(OhNo::TriedLoadingOnceLang {
          lang: lang_name.to_owned(),
        }),
      })
  }
}
