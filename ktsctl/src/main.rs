use std::collections::HashSet;

use clap::Parser;
use cli::Cli;
use colored::Colorize;
use error::HellNo;
use kak_tree_sitter_config::Config;

use crate::{
  commands::{
    manage::{ManageFlags, Manager},
    query::Query,
    remove,
  },
  resources::Resources,
};

mod cli;
mod commands;
mod error;
mod git;
mod process;
mod resources;
mod ui;

fn main() {
  if let Err(err) = start() {
    eprintln!("{}", err.to_string().red());
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
    cli::Cmd::Fetch { all, lang } => {
      let flags = ManageFlags::new(true, false, false, false);
      manage(config, flags, all, lang.as_deref())?
    }

    cli::Cmd::Compile { all, lang } => {
      let flags = ManageFlags::new(false, true, false, false);
      manage(config, flags, all, lang.as_deref())?
    }

    cli::Cmd::Install { all, lang } => {
      let flags = ManageFlags::new(false, false, true, false);
      manage(config, flags, all, lang.as_deref())?
    }

    cli::Cmd::Sync { all, lang } => {
      let flags = ManageFlags::new(false, false, false, true);
      manage(config, flags, all, lang.as_deref())?
    }

    cli::Cmd::Query { lang, all } => {
      let query = Query::new(config)?;
      if let Some(lang) = lang {
        let sections = query.lang_info_sections(lang.as_str());
        for sct in sections {
          println!("{sct}");
        }
      } else if all {
        let all_tbl = query.all_lang_info_tbl();
        println!("{all_tbl}");
      }
    }

    cli::Cmd::Remove {
      mut grammar,
      mut queries,
      prune,
      lang,
    } => {
      let resources = Resources::new()?;
      if !grammar && !queries {
        grammar = true;
        queries = true;
      }

      remove::remove(&config, &resources, grammar, queries, prune, lang)?;
    }

    cli::Cmd::Prune => {
      let resources = Resources::new()?;
      remove::prune_unpinned(&config, &resources)?;
    }
  }

  Ok(())
}

fn manage(
  config: Config,
  manage_flags: ManageFlags,
  all: bool,
  lang: Option<&str>,
) -> Result<(), HellNo> {
  if let Some(lang) = lang {
    let manager = Manager::new(config, manage_flags)?;
    manager.manage(lang)?;
  } else if all {
    let all_langs: HashSet<_> = config.languages.language.keys().cloned().collect();
    let manager = Manager::new(config, manage_flags)?;
    manager.manage_all(all_langs.iter().map(|s| s.as_str()));
  }

  Ok(())
}
