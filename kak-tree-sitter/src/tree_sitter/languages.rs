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
use tree_house::{highlighter::Highlight, text_object::TextObjectQuery};
use tree_house_bindings::Query;

use crate::{error::OhNo, kakoune::face::Face, tree_sitter::queries::Queries};

pub struct Language {
  pub name: String,
  pub language: tree_house::Language,
  // query to use for text objects, if supported by the language
  pub textobject_query: Option<TextObjectQuery>,

  lang_config: tree_house::LanguageConfig,
}

impl Language {
  pub fn lang_name(&self) -> &str {
    &self.name
  }

  pub fn language(&self) -> tree_house::Language {
    self.language
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

  /// Reverse lookup mapping a numeric ID (Language) to its langage name.
  lang_ids: Vec<String>,

  /// List of faces; used to resolve face names from their indices.
  faces: Rc<Vec<Face>>,
}

type LazyLang = LazyCell<CachedLanguage, Box<dyn FnOnce() -> CachedLanguage + 'static>>;
pub type Grammar2Cache = Rc<RefCell<HashMap<String, tree_house_bindings::Grammar>>>;

impl Languages {
  pub fn new(config: &Config) -> Self {
    let mut hl_names: Vec<_> = config.highlight.groups.iter().cloned().collect();

    // NOTE: sorting in descending order allows to ensure that we will always match against the longest (more accurate)
    // capture groups first; even though we have `punctuation`, parenthesis match `punctuation.bracket` and commas
    // match `punctuation.delimiter`, so we want to resolve them as the more accurate capture group for better
    // highlighting support.
    hl_names.sort_by(|a, b| b.cmp(a));

    let faces = Rc::new(hl_names.iter().map(Face::from_capture_group).collect());
    let hl_names = Rc::new(hl_names.clone());

    let grammars2: Grammar2Cache = Rc::new(RefCell::new(HashMap::new()));

    let lang_list: Vec<_> = config
      .languages
      .iter()
      .zip(0..)
      .map(|((lang_name, _), idx)| {
        let config = config.clone();
        let hl_names = hl_names.clone();
        let lang_name2 = lang_name.to_owned();
        let grammars2 = grammars2.clone();

        let lazy = LazyLang::new(Box::new(move || {
          match Self::load_lang(
            &hl_names,
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

    Self {
      langs,
      lang_ids,
      faces,
    }
  }

  fn load_grammar(
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
    hl_names: &[String],
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
    let grammar = if let Some(grammar) = grammars2.borrow().get(lang_name) {
      log::debug!("grammar {lang_name} already loaded; using cached version");
      tree_house_bindings::Grammar::clone(grammar)
    } else {
      let grammar = Self::load_grammar(grammar_name, grammar_config)?;
      grammars2.borrow_mut().insert(lang_name.to_owned(), grammar);
      grammar
    };

    let queries = Self::load_queries(lang_name, lang_config)?;

    // tree-sitter-highlight configuration
    let textobject_query = queries
      .text_objects
      .as_deref()
      .map(|q| {
        Query::new(grammar, q, |_pat, _pred| Ok(())).map(|query| Some(TextObjectQuery { query }))
      })
      .unwrap_or_else(|| Ok(None))?;

    // tree-house configuration
    let lang_config = tree_house::LanguageConfig::new(
      grammar,
      queries.highlights.as_deref().unwrap_or(""),
      queries.injections.as_deref().unwrap_or(""),
      queries.locals.as_deref().unwrap_or(""),
    )?;

    lang_config.configure(|name| {
      hl_names
        .iter()
        .position(|hl_name| name.starts_with(hl_name))
        .map(|idx| Highlight::new(idx as _))
    });

    let lang = Language {
      name: lang_name.to_owned(),
      language,
      textobject_query,
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

  pub fn faces(&self) -> &Rc<Vec<Face>> {
    &self.faces
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
