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
    cli::Cmd::Fetch { all, langs } => {
      let flags = ManageFlags::new(true, false, false, false);
      manage(config, flags, all, langs)?
    }

    cli::Cmd::Compile { all, langs } => {
      let flags = ManageFlags::new(false, true, false, false);
      manage(config, flags, all, langs)?
    }

    cli::Cmd::Install { all, langs } => {
      let flags = ManageFlags::new(false, false, true, false);
      manage(config, flags, all, langs)?
    }

    cli::Cmd::Sync { all, langs } => {
      let flags = ManageFlags::new(false, false, false, true);
      manage(config, flags, all, langs)?
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
      if !grammar && !queries {
        grammar = true;
        queries = true;
      }

      remove::remove(
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
      remove::prune_unpinned(&config, &resources)?;
    }
  }

  Ok(())
}

fn manage(
  config: Config,
  manage_flags: ManageFlags,
  all: bool,
  langs: Vec<String>,
) -> Result<(), HellNo> {
  // no language passed and all used; synchronize everything known in the configuration
  if langs.is_empty() && all {
    let all_langs: HashSet<_> = config.languages.language.keys().cloned().collect();
    let manager = Manager::new(config, manage_flags)?;
    manager.manage_all(all_langs.iter().map(|s| s.as_str()));
  } else {
    for lang in langs {
      if let Err(err) = manage_lang(config.clone(), manage_flags.clone(), &lang) {
        log::error!("{err}");
      }
    }
  }

  Ok(())
}

fn manage_lang(config: Config, manage_flags: ManageFlags, lang: &str) -> Result<(), HellNo> {
  let manager = Manager::new(config, manage_flags)?;
  manager.manage(lang)
}
