//! Manage sub-commands.

use std::{fs, io, path::Path};

use kak_tree_sitter_config::{Config, GrammarConfig, LanguageConfig, source::Source};

use crate::{
  error::HellNo,
  git::{self, Clone},
  process::Process,
  resources::Resources,
  ui::report::Report,
};

/// Main flags to fetch, compile and/or install resources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManageFlags {
  pub fetch: bool,
  pub compile: bool,
  pub install: bool,
  pub sync: bool,
}

impl ManageFlags {
  pub fn new(fetch: bool, compile: bool, install: bool, sync: bool) -> Self {
    Self {
      fetch,
      compile,
      install,
      sync,
    }
  }
}

#[derive(Debug)]
pub struct Manager {
  config: Config,
  flags: ManageFlags,
  resources: Resources,
}

impl Manager {
  pub fn new(config: Config, flags: ManageFlags) -> Result<Self, HellNo> {
    let resources = Resources::new()?;

    Ok(Self {
      config,
      flags,
      resources,
    })
  }

  pub fn manage(&self, report: Report, lang: &str) -> Result<(), HellNo> {
    let lang_config = self.config.languages.get_lang_config(lang)?;
    let grammar_lang = lang_config.grammar.as_deref().unwrap_or(lang);
    let grammar_config = self.config.grammars.get_grammar_config(grammar_lang)?;

    report!(report, "working {}", lang.blue());
    self.manage_grammar(report.incr(), grammar_lang, grammar_config)?;
    self.manage_queries(report.incr(), lang, lang_config)
  }

  pub fn manage_all<'a>(&self, report: Report, langs: impl Iterator<Item = &'a str>) {
    for lang in langs {
      let r = self.manage(report.incr(), lang);

      if let Err(err) = r {
        report_error!(report, "{err}");
      }
    }
  }

  fn manage_grammar(
    &self,
    report: Report,
    lang: &str,
    grammar_config: &GrammarConfig,
  ) -> Result<(), HellNo> {
    match grammar_config.source {
      Source::Local { ref path } => {
        report_info!(
          report,
          "using local grammar {lang} at {path}",
          path = path.display()
        );
      }

      Source::Git { ref url, ref pin } => {
        self.manage_git_grammar(report, lang, grammar_config, url, pin)?
      }
    }

    Ok(())
  }

  fn manage_git_grammar(
    &self,
    report: Report,
    lang: &str,
    grammar_config: &GrammarConfig,
    url: &str,
    pin: &str,
  ) -> Result<(), HellNo> {
    let sources_path = self.resources.sources_dir(url);

    if self.flags.sync {
      report!(report, "synchronizing {lang} grammar");
      let report = report.incr();

      self.sync_git_grammar(report, lang, grammar_config, &sources_path, url, pin)?;
      return Ok(());
    }

    if self.flags.fetch {
      report!(report, "fetching {lang} grammar", lang = lang.blue());
      let report = report.incr();

      let clone = Self::git_clone(report, lang, &sources_path, url, pin)?;

      if let Clone::Cloned = clone {
        report_success!(report, "cloned {lang} grammar");
      } else {
        report_info!(report, "{lang} grammar was already cloned (cached)");
      }

      return Ok(());
    }

    let lang_build_dir = self
      .resources
      .lang_build_dir(&sources_path, &grammar_config.path);

    if self.flags.compile {
      report!(report, "compiling {lang} grammar");
      let report = report.incr();

      Self::compile_git_grammar(report, lang, grammar_config, &lang_build_dir)?;
      report_success!(report, "built {lang} grammar");

      return Ok(());
    }

    if self.flags.install {
      report_info!(report, "installing {lang} grammar");
      let report = report.incr();

      self.install_git_grammar(report, lang, &lang_build_dir, pin)?;
      report_success!(report, "installed {lang} grammar");
    }

    Ok(())
  }

  fn sync_git_grammar(
    &self,
    report: Report,
    lang: &str,
    grammar_config: &GrammarConfig,
    fetch_path: &Path,
    url: &str,
    pin: &str,
  ) -> Result<(), HellNo> {
    if self.resources.grammar_exists(lang, pin) {
      report_info!(
        report,
        "grammar {lang} already installed ({pin})",
        lang = lang.blue(),
        pin = pin.yellow()
      );
      return Ok(());
    }

    Self::git_clone(report, lang, fetch_path, url, pin)?;

    let lang_build_dir = self
      .resources
      .lang_build_dir(fetch_path, &grammar_config.path);

    Self::compile_git_grammar(report, lang, grammar_config, &lang_build_dir)?;
    self.install_git_grammar(report, lang, &lang_build_dir, pin)?;

    report_success!(report, "synchronized {lang} grammar", lang = lang.blue());
    Ok(())
  }

  fn git_clone(
    report: Report,
    lang: &str,
    fetch_path: &Path,
    url: &str,
    pin: &str,
  ) -> Result<Clone, HellNo> {
    let clone = git::clone(report, fetch_path, url)?;

    if let git::Clone::Cloned = clone {
      report_info!(
        report,
        "cloned {lang} at {path}",
        path = fetch_path.display(),
      );
    } else {
      report_info!(
        report,
        "already cloned {lang} at {path} (cached)",
        path = fetch_path.display(),
      );
    }

    git::fetch(report, lang, fetch_path, url, pin)?;
    Ok(clone)
  }

  /// Compile and link the grammar.
  fn compile_git_grammar(
    report: Report,
    lang: &str,
    grammar_config: &GrammarConfig,
    lang_build_dir: &Path,
  ) -> Result<(), HellNo> {
    report!(report, "compiling {lang} grammar");

    // ensure the build dir exists
    fs::create_dir_all(lang_build_dir).map_err(|err| HellNo::CannotCreateDir {
      dir: lang_build_dir.to_owned(),
      err,
    })?;

    // compile
    let args: Vec<_> = grammar_config
      .compile_args
      .iter()
      .map(|x| x.as_str())
      .chain(grammar_config.compile_flags.iter().map(|x| x.as_str()))
      .collect();

    Process::new(&grammar_config.compile).run(lang_build_dir, &args)?;

    report_success!(report.incr(), "compiled {lang} grammar");

    // link into {lang}.so
    report!(report, "linking {lang} grammar",);

    let link_args: Vec<_> = grammar_config
      .link_args
      .iter()
      .map(|x| x.replace("{lang}", lang))
      .collect();
    let args: Vec<_> = link_args
      .iter()
      .map(|x| x.as_str())
      .chain(grammar_config.link_flags.iter().map(|x| x.as_str()))
      .collect();
    Process::new(&grammar_config.link).run(lang_build_dir, &args)?;

    report_success!(report.incr(), "linked {lang} grammar");
    Ok(())
  }

  fn install_git_grammar(
    &self,
    report: Report,
    lang: &str,
    lang_build_dir: &Path,
    pin: &str,
  ) -> Result<(), HellNo> {
    report!(report, "installing {lang} grammar");

    let lang_so = format!("{lang}.so");
    let source_path = lang_build_dir.join(lang_so);
    let grammar_dir = self.resources.data_dir().join(format!("grammars/{lang}"));
    let install_path = grammar_dir.join(format!("{pin}.so"));

    // ensure the grammars directory exists
    fs::create_dir_all(&grammar_dir).map_err(|err| HellNo::CannotCreateDir {
      dir: grammar_dir,
      err,
    })?;
    fs::copy(&source_path, &install_path).map_err(|err| HellNo::CannotCopyFile {
      src: source_path,
      dest: install_path,
      err,
    })?;

    report_success!(report.incr(), "installed {lang} grammar");
    Ok(())
  }

  fn manage_queries(
    &self,
    report: Report,
    lang: &str,
    lang_config: &LanguageConfig,
  ) -> Result<(), HellNo> {
    match lang_config.queries.source {
      Some(Source::Local { ref path }) => {
        report_info!(
          report,
          "using local queries {lang} at {path}",
          path = path.display()
        );
      }

      Some(Source::Git { ref url, ref pin }) => {
        self.manage_git_queries(report, lang, lang_config, url, pin)?
      }

      None => {
        report_warn!(
          report,
          "no query configuration for {lang}; will be using the grammar directory"
        );
      }
    }

    Ok(())
  }

  fn manage_git_queries(
    &self,
    report: Report,
    lang: &str,
    lang_config: &LanguageConfig,
    url: &str,
    pin: &str,
  ) -> Result<(), HellNo> {
    let sources_path = self.resources.sources_dir(url);

    if self.flags.sync {
      report!(report, "synchronizing {lang} queries");
      self.sync_git_queries(report.incr(), lang, lang_config, &sources_path, url, pin)?;
      return Ok(());
    }

    if self.flags.fetch {
      report!(report, "cloning {lang} queries");
      let clone = Self::git_clone(report.incr(), lang, &sources_path, url, pin)?;

      if let Clone::Cloned = clone {
        report_success!(report, "cloned {lang} queries");
      } else {
        report_info!(report, "{lang} queries were already cloned (cached)");
      }

      return Ok(());
    }

    if self.flags.install {
      report!(report, "installing {lang} queries");
      let query_dir = sources_path.join(&lang_config.queries.path);
      self.install_git_queries(report.incr(), &query_dir, lang, pin)?;
    }

    Ok(())
  }

  fn sync_git_queries(
    &self,
    report: Report,
    lang: &str,
    lang_config: &LanguageConfig,
    fetch_path: &Path,
    url: &str,
    pin: &str,
  ) -> Result<(), HellNo> {
    if self.resources.queries_exist(lang, pin) {
      report_info!(
        report,
        "queries {lang} already installed ({pin})",
        lang = lang.blue(),
        pin = pin.yellow()
      );
      return Ok(());
    }

    Self::git_clone(report, lang, fetch_path, url, pin)?;

    let path = lang_config.queries.normalized_path(lang);
    let query_dir = fetch_path.join(path);
    self.install_git_queries(report, &query_dir, lang, pin)?;

    report_success!(report, "synchronized {lang} queries", lang = lang.blue());
    Ok(())
  }

  fn install_git_queries(
    &self,
    report: Report,
    query_dir: &Path,
    lang: &str,
    pin: &str,
  ) -> Result<(), HellNo> {
    report!(report, "installing {lang} queries");

    // ensure the queries directory exists
    let install_path = self.resources.queries_pin_dir(lang, pin);

    fs::create_dir_all(&install_path).map_err(|err| HellNo::CannotCreateDir {
      dir: install_path.clone(),
      err,
    })?;

    Self::copy_dir(query_dir, &install_path).map_err(|err| HellNo::CannotCopyDir {
      src: query_dir.to_owned(),
      dest: install_path,
      err,
    })?;

    report_success!(report.incr(), "installed {lang} queries");
    Ok(())
  }

  fn copy_dir(from: &Path, to: &Path) -> Result<(), io::Error> {
    for entry in from.read_dir()?.flatten() {
      let new_to = to.join(entry.file_name());

      if entry.file_type()?.is_file() {
        fs::copy(entry.path(), &new_to)?;
      }
    }

    Ok(())
  }
}
