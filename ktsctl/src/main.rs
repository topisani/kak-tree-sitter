use std::collections::HashSet;

use clap::Parser;
use cli::Cli;
use error::HellNo;
use kak_tree_sitter_config::Config;

use crate::{
  commands::{
    manage::{ManageFlags, Manager},
    query::Query,
    remove,
  },
  resources::Resources,
  ui::report::Report,
};

#[macro_use]
mod ui;
mod cli;
mod commands;
mod error;
mod git;
mod process;
mod resources;

fn main() {
  if let Err(err) = start() {
    report_error!(Report::new(), "{}", err.to_string().red());
    std::process::exit(1);
  }
}

fn start() -> Result<(), HellNo> {
  let cli = Cli::parse();

  if cli.verbose {
    simple_logger::init_with_level(log::Level::Debug)?;
  }

  let config = if let Some(path_config) = &cli.config {
    Config::load_user(path_config)?
  } else {
    Config::load_from_xdg()?
  };
  log::debug!("ktsctl configuration:\n{config:#?}");

  match cli.cmd {
    cli::Cmd::Fetch { all, langs } => {
      let flags = ManageFlags::new(true, false, false, false);

      let report = Report::new();
      report!(report, "fetching languages: {langs:?}");
      manage(report.incr(), config, flags, all, langs)?
    }

    cli::Cmd::Compile { all, langs } => {
      let flags = ManageFlags::new(false, true, false, false);

      let report = Report::new();
      report!(report, "compiling languages: {langs:?}");
      manage(report.incr(), config, flags, all, langs)?
    }

    cli::Cmd::Install { all, langs } => {
      let flags = ManageFlags::new(false, false, true, false);

      let report = Report::new();
      report!(report, "installing languages: {langs:?}");
      manage(report.incr(), config, flags, all, langs)?
    }

    cli::Cmd::Sync { all, langs } => {
      let flags = ManageFlags::new(false, false, false, true);

      let report = Report::new();
      report!(report, "synchronizing languages: {langs:?}");
      manage(report.incr(), config, flags, all, langs)?
    }

    cli::Cmd::Query { lang, all } => {
      let query = Query::new(config)?;
      if let Some(lang) = lang {
        let sections = query.lang_info_sections(lang.as_str());
        for sct in sections {
          println!("{sct}");
        }
      } else if all {
        let all_tbl = query.all_lang_info_tbl()?;
        println!("{all_tbl}");
      }
    }

    cli::Cmd::Remove {
      mut grammar,
      mut queries,
      prune,
      langs,
    } => {
      let resources = Resources::new()?;

      // if none of grammar and queries are provided, we assume we want to delete everything
      if !grammar && !queries {
        grammar = true;
        queries = true;
      }

      let report = Report::new();
      report!(report, "removing languages: {langs:?}");
      remove::remove(
        report.incr(),
        &config,
        &resources,
        grammar,
        queries,
        prune,
        langs.iter().map(String::as_str),
      );
    }

    cli::Cmd::Prune => {
      let resources = Resources::new()?;

      let report = Report::new();
      report!(report, "pruning");
      remove::prune_unpinned(report.incr(), &config, &resources)?;
    }
    cli::Cmd::DefaultConfig => {
      println!("{}", Config::DEFAULT_CONFIG_CONTENT);
    }
  }

  Ok(())
}

fn manage(
  report: Report,
  config: Config,
  manage_flags: ManageFlags,
  all: bool,
  langs: Vec<String>,
) -> Result<(), HellNo> {
  // no language passed and all used; synchronize everything known in the configuration
  if langs.is_empty() && all {
    let all_langs: HashSet<_> = config.languages.language.keys().cloned().collect();
    let manager = Manager::new(config, manage_flags)?;
    manager.manage_all(report, all_langs.iter().map(|s| s.as_str()));
  } else {
    for lang in langs {
      if let Err(err) = manage_lang(report, config.clone(), manage_flags.clone(), &lang) {
        report_error!(report, "{err}", err = err.to_string().red());
      }
    }
  }

  Ok(())
}

fn manage_lang(
  report: Report,
  config: Config,
  manage_flags: ManageFlags,
  lang: &str,
) -> Result<(), HellNo> {
  let manager = Manager::new(config, manage_flags)?;
  manager.manage(report, lang)
}
