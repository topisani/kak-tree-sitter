//! Module to remove resources.

use std::fs;

use colored::Colorize;
use kak_tree_sitter_config::{Config, GrammarConfig, LanguageConfig, source::Source};

use crate::{
  error::HellNo,
  resources::Resources,
  ui::{report::Report, status_icon::StatusIcon},
};

/// Delete resources associated with a given language.
pub fn remove<'lang>(
  config: &Config,
  resources: &Resources,
  grammar: bool,
  queries: bool,
  prune: bool,
  langs: impl Iterator<Item = &'lang str>,
) {
  for lang in langs {
    if let Err(err) = remove_lang(config, resources, grammar, queries, prune, lang) {
      log::error!("{err}");
    }
  }
}

/// Delete resources associated with a given language.
pub fn remove_lang(
  config: &Config,
  resources: &Resources,
  grammar: bool,
  queries: bool,
  prune: bool,
  lang: &str,
) -> Result<(), HellNo> {
  let lang_config = config.languages.get_lang_config(lang)?;
  let grammar_config = config
    .grammars
    .get_grammar_config(lang_config.grammar.as_deref().unwrap_or(lang))?;
  let report = Report::new(StatusIcon::Sync, format!("removing resources for {lang}"));
  let mut errors = Vec::new();

  if grammar {
    remove_grammar(resources, lang, grammar_config, prune, &report, &mut errors);
  }

  if queries {
    remove_queries(resources, lang, lang_config, prune, &report, &mut errors);
  }

  if errors.is_empty() {
    report.success(format!("{lang} removed"));
  } else {
    report.error(format!("cannot remove {lang}"));

    for err in errors {
      eprintln!("{}", err.red());
    }
  }

  Ok(())
}

fn remove_grammar(
  resources: &Resources,
  lang: &str,
  grammar_config: &GrammarConfig,
  prune: bool,
  report: &Report,
  errors: &mut Vec<String>,
) {
  if prune {
    let dir = resources.grammars_dir(lang);
    if let Ok(true) = dir.try_exists() {
      report.info(format!("removing {lang} grammar"));

      if let Err(err) = fs::remove_dir_all(dir) {
        errors.push(format!("cannot remove {lang} grammar: {err}"));
      }
    }
  } else {
    let grammar_path = resources.grammar_path_from_config(lang, grammar_config);

    if let Ok(true) = grammar_path.try_exists() {
      report.info(format!("removing {lang} grammar"));

      if let Err(err) = fs::remove_file(grammar_path) {
        errors.push(format!("cannot remove {lang} grammar: {err}"));
      }
    }
  }
}

fn remove_queries(
  resources: &Resources,
  lang: &str,
  lang_config: &LanguageConfig,
  prune: bool,
  report: &Report,
  errors: &mut Vec<String>,
) {
  let dir = if prune {
    Some(resources.queries_dir(lang))
  } else {
    resources.queries_dir_from_config(lang, lang_config)
  };

  if let Some(dir) = dir {
    if let Ok(true) = dir.try_exists() {
      report.info(format!("removing {lang} queries"));

      if let Err(err) = fs::remove_dir_all(dir) {
        errors.push(format!("cannot remove {lang} queries: {err}"));
      }
    }
  }
}

/// Prune everything, removing unpinned data.
pub fn prune_unpinned(config: &Config, resources: &Resources) -> Result<(), HellNo> {
  for (lang, lang_config) in config.languages.iter() {
    let grammar_config = config
      .grammars
      .get_grammar_config(lang_config.grammar.as_deref().unwrap_or(lang))?;
    let mut errors = Vec::new();
    let report = Report::new(StatusIcon::Info, "pruning");
    report.info(format!("pruning {}", lang.blue()));

    if let Err(err) = prune_unpinned_lang(resources, lang, lang_config, grammar_config, &mut errors)
    {
      errors.push(format!("{err}"));
    }

    if errors.is_empty() {
      report.success(format!("pruned {}", lang.blue()));
    } else {
      report.error(format!(
        "cannot prune {lang}:\n  {err}",
        err = errors.join("\n  ")
      ));
    }
  }

  Ok(())
}

fn prune_unpinned_lang(
  resources: &Resources,
  lang: &str,
  lang_config: &LanguageConfig,
  grammar_config: &GrammarConfig,
  errors: &mut Vec<String>,
) -> Result<(), HellNo> {
  prune_unpinned_grammar(resources, lang, grammar_config, errors)?;
  prune_unpinned_queries(resources, lang, lang_config, errors)?;
  Ok(())
}

fn prune_unpinned_grammar(
  resources: &Resources,
  lang: &str,
  grammar_config: &GrammarConfig,
  errors: &mut Vec<String>,
) -> Result<(), HellNo> {
  let grammar_dir = resources.grammars_dir(lang);
  let grammar_dir = grammar_dir
    .read_dir()
    .map_err(|_| HellNo::NoGrammarDirForLang {
      lang: lang.to_owned(),
    })?;

  for entry in grammar_dir.flatten() {
    match &grammar_config.source {
      Source::Local { path } => {
        if entry.path() != *path {
          if let Err(err) = fs::remove_file(entry.path()) {
            errors.push(format!(
              "cannot prune grammar for {lang} at {path}: {err}",
              path = entry.path().display()
            ));
          }
        }
      }

      Source::Git { pin, .. } => {
        if !entry
          .path()
          .file_name()
          .unwrap()
          .to_str()
          .unwrap()
          .starts_with(pin)
        {
          if let Err(err) = fs::remove_file(entry.path()) {
            errors.push(format!(
              "cannot prune grammar for {lang} at {path}: {err}",
              path = entry.path().display()
            ));
          }
        }
      }
    }
  }

  Ok(())
}

fn prune_unpinned_queries(
  resources: &Resources,
  lang: &str,
  lang_config: &LanguageConfig,
  errors: &mut Vec<String>,
) -> Result<(), HellNo> {
  let queries_dir = resources.queries_dir(lang);
  let queries_dir = queries_dir
    .read_dir()
    .map_err(|_| HellNo::NoQueriesDirForLang {
      lang: lang.to_owned(),
    })?;

  for entry in queries_dir.flatten() {
    match &lang_config.queries.source {
      Some(Source::Local { path }) => {
        if entry.path() != *path {
          if let Err(err) = fs::remove_file(entry.path()) {
            errors.push(format!(
              "cannot prune queries for {lang} at {path}: {err}",
              path = entry.path().display()
            ));
          }
        }
      }

      Some(Source::Git { pin, .. }) => {
        if entry.path().file_name().unwrap().to_str() != Some(pin) {
          if let Err(err) = fs::remove_dir_all(entry.path()) {
            errors.push(format!(
              "cannot prune queries for {lang} at {path}: {err}",
              path = entry.path().display()
            ));
          }
        }
      }

      // no source means that we use the same as the grammar
      None => todo!(),
    }
  }

  Ok(())
}
