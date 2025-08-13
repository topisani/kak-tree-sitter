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
  pub language: tree_house::Language,
  pub hl_config: HighlightConfiguration,
  pub hl_names: Vec<String>,
  // query to use for text objects, if supported by the language
  pub textobject_query: Option<Query>,

  #[deprecated = "use grammar2, which is using tree_house_bindings"]
  grammar: Rc<Grammar>,
  lang_config: tree_house::LanguageConfig,
}

impl Language {
  pub fn lang_name(&self) -> &str {
    &self.name
  }

  pub fn language(&self) -> tree_house::Language {
    self.language
  }

  pub fn grammar(&self) -> &Rc<Grammar> {
    &self.grammar
  }

  pub fn lang_config(&self) -> &tree_house::LanguageConfig {
    &self.lang_config
  }
}

#[deprecated = "use tree_house_bindings::Grammar instead"]
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

  /// Reverse lookup mapping a numeric idea (Language)
  lang_ids: Vec<String>,
}

type LazyLang = LazyCell<CachedLanguage, Box<dyn FnOnce() -> CachedLanguage + 'static>>;
pub type GrammarCache = Rc<RefCell<HashMap<String, Rc<Grammar>>>>;
pub type Grammar2Cache = Rc<RefCell<HashMap<String, tree_house_bindings::Grammar>>>;

impl Languages {
  pub fn new(config: &Config) -> Self {
    let grammars: GrammarCache = Rc::new(RefCell::new(HashMap::new()));
    let grammars2: Grammar2Cache = Rc::new(RefCell::new(HashMap::new()));

    let lang_list: Vec<_> = config
      .languages
      .iter()
      .zip(0..)
      .map(|((lang_name, _), idx)| {
        let config = config.clone();
        let lang_name2 = lang_name.to_owned();
        let grammars = grammars.clone();
        let grammars2 = grammars2.clone();

        let lazy = LazyLang::new(Box::new(move || {
          match Self::load_lang(
            &grammars,
            &grammars2,
            &config,
            &lang_name2,
            tree_house::Language(idx),
          ) {
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
    let lang_ids = lang_list.iter().map(|(name, _)| name.clone()).collect();
    let langs = lang_list.into_iter().collect();

    Self { langs, lang_ids }
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

  fn load_grammar2(
    lang_name: &str,
    grammar_config: &GrammarConfig,
  ) -> Result<tree_house_bindings::Grammar, OhNo> {
    let Some(path) = GrammarsConfig::get_grammar_path(grammar_config, lang_name) else {
      return Err(OhNo::CannotLoadGrammar2 {
        lang: lang_name.to_owned(),
        err: format!("no grammar path for language {lang_name}"),
      });
    };
    log::debug!("  grammar path: {}", path.display());

    let grammar = unsafe {
      tree_house_bindings::Grammar::new(lang_name, &path).map_err(|err| {
        OhNo::CannotLoadGrammar2 {
          lang: lang_name.to_owned(),
          err: err.to_string(),
        }
      })?
    };

    Ok(grammar)
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
  fn load_lang(
    grammars: &GrammarCache,
    grammars2: &Grammar2Cache,
    config: &Config,
    lang_name: impl AsRef<str>,
    language: tree_house::Language,
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
    let grammar2 = if let Some(grammar) = grammars2.borrow().get(lang_name) {
      log::debug!("grammar {lang_name} alread loaded; using cached version");
      tree_house_bindings::Grammar::clone(grammar)
    } else {
      let grammar = Self::load_grammar2(grammar_name, grammar_config)?;

      grammars2.borrow_mut().insert(lang_name.to_owned(), grammar);

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

    let lang_config = tree_house::LanguageConfig::new(
      grammar2,
      queries.highlights.as_deref().unwrap_or(""),
      queries.injections.as_deref().unwrap_or(""),
      queries.locals.as_deref().unwrap_or(""),
    )?;

    let lang = Language {
      name: lang_name.to_owned(),
      language,
      hl_config,
      hl_names,
      textobject_query,
      grammar,
      lang_config,
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

impl tree_house::LanguageLoader for Languages {
  fn language_for_marker(
    &self,
    marker: tree_house::InjectionLanguageMarker,
  ) -> Option<tree_house::Language> {
    match marker {
      tree_house::InjectionLanguageMarker::Name(name) => {
        self.get(name).ok().map(|lang| lang.language)
      }

      tree_house::InjectionLanguageMarker::Match(name)
      | tree_house::InjectionLanguageMarker::Filename(name)
      | tree_house::InjectionLanguageMarker::Shebang(name) => {
        self.get(name.as_str()?).ok().map(|lang| lang.language)
      }
    }
  }

  fn get_config(&self, lang: tree_house::Language) -> Option<&tree_house::LanguageConfig> {
    let lang_name = self.lang_ids.get(lang.0 as usize)?.as_str();
    self.get(lang_name).ok().map(|lang| &lang.lang_config)
  }
}
