//! Get information about configuration and installed resources.

use std::{collections::HashSet, iter, path::Path};

use colored::Colorize;
use kak_tree_sitter_config::{Config, GrammarConfig, LanguageConfig};

use crate::{
  error::HellNo,
  resources::Resources,
  ui::{
    section::{Field, FieldValue, Section, SectionBuilder},
    source::source_field,
    table::{Cell, Row, RowBuilder, Table},
  },
};

/// Main source of query.
#[derive(Debug)]
pub struct Query {
  config: Config,
  resources: Resources,
}

impl Query {
  pub fn new(config: Config) -> Result<Self, HellNo> {
    let resources = Resources::new()?;
    Ok(Self { config, resources })
  }

  /// A table representing all language information.
  pub fn all_lang_info_tbl(&self) -> Result<Table, HellNo> {
    fn check_path_sign(path: &Path) -> Cell {
      if let Ok(true) = path.try_exists() {
        Cell::new("")
      } else {
        Cell::new("")
      }
    }

    let mut table = Table::default();
    table.header(
      RowBuilder::default()
        .push(Cell::new("Language".bold()))
        .push(Cell::new("Grammar".bold()))
        .push(Cell::new("Highlights".bold()))
        .push(Cell::new("Injections".bold()))
        .push(Cell::new("Locals".bold()))
        .push(Cell::new("Text-objects".bold()))
        .push(Cell::new("Indents".bold()))
        .build(),
    );

    let mut langs = self.config.languages.language.iter().collect::<Vec<_>>();
    langs.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (lang, lang_config) in langs {
      let grammar_config = self
        .config
        .grammars
        .get_grammar_config(lang_config.grammar.as_deref().unwrap_or(lang))?;
      let grammar_path = self
        .resources
        .grammar_path_from_config(lang, grammar_config);

      let mut row = Row::default();
      row.push(lang.as_str());
      row.push(check_path_sign(&grammar_path));

      if let Some(queries_path) = self.resources.queries_dir_from_config(lang, lang_config) {
        row.push(check_path_sign(&queries_path.join("highlights.scm")));
        row.push(check_path_sign(&queries_path.join("injections.scm")));
        row.push(check_path_sign(&queries_path.join("locals.scm")));
        row.push(check_path_sign(&queries_path.join("textobjects.scm")));
        row.push(check_path_sign(&queries_path.join("indents.scm")));
      } else {
        for _ in 0..5 {
          row.push(Cell::new(""));
        }
      }

      table.push(row);
    }

    Ok(table)
  }

  /// Sections providing information about a given language.
  pub fn lang_info_sections(&self, lang: &str) -> Vec<Section> {
    let Ok(lang_config) = self.config.languages.get_lang_config(lang) else {
      return Vec::default();
    };

    let Ok(grammar_config) = self
      .config
      .grammars
      .get_grammar_config(lang_config.grammar.as_deref().unwrap_or(lang))
    else {
      return Vec::default();
    };

    self
      .lang_config_sections(lang, lang_config, grammar_config)
      .chain(iter::once(self.lang_install_stats_section(
        lang,
        lang_config,
        grammar_config,
      )))
      .collect()
  }

  fn lang_config_sections(
    &self,
    lang_name: &str,
    lang_config: &LanguageConfig,
    grammar_config: &GrammarConfig,
  ) -> impl Iterator<Item = Section> + use<> {
    [
      self.lang_config_grammar_section(grammar_config),
      self.lang_config_queries_section(lang_name, lang_config),
    ]
    .into_iter()
  }

  fn lang_install_stats_section(
    &self,
    lang: &str,
    lang_config: &LanguageConfig,
    grammar_config: &GrammarConfig,
  ) -> Section {
    let mut section = Section::new("Install stats");
    self.grammar_fields(&mut section, lang, grammar_config);
    self.queries_fields(&mut section, lang, lang_config);
    section
  }

  fn lang_config_grammar_section(&self, grammar_config: &GrammarConfig) -> Section {
    let compile_field_value: Vec<_> = iter::once(grammar_config.compile.green())
      .chain(grammar_config.compile_args.iter().map(|x| x.green()))
      .collect();
    let link_field_value: Vec<_> = iter::once(grammar_config.link.green())
      .chain(grammar_config.link_args.iter().map(|x| x.green()))
      .collect();

    SectionBuilder::new("Grammar configuration")
      .push(source_field(&grammar_config.source))
      .push(Field::kv(
        "Path".blue(),
        grammar_config.path.display().to_string().green(),
      ))
      .push(Field::kv(
        "Compilation command".blue(),
        FieldValue::list(compile_field_value),
      ))
      .push(Field::kv(
        "Compilation flags".blue(),
        FieldValue::list(
          grammar_config
            .compile_flags
            .iter()
            .map(|x| x.green())
            .collect::<Vec<_>>(),
        ),
      ))
      .push(Field::kv(
        "Link command".blue(),
        FieldValue::list(link_field_value),
      ))
      .push(Field::kv(
        "Link flags".blue(),
        FieldValue::list(
          grammar_config
            .link_flags
            .iter()
            .map(|x| x.green())
            .collect::<Vec<_>>(),
        ),
      ))
      .build()
  }

  fn lang_config_queries_section(&self, lang_name: &str, lang_config: &LanguageConfig) -> Section {
    let queries = &lang_config.queries;

    let mut section = Section::new("Queries configuration");

    if let Some(ref source) = queries.source {
      section.push(source_field(source));
    }

    section
      .push(Field::kv(
        "Path".blue(),
        queries
          .normalized_path(lang_name)
          .display()
          .to_string()
          .green(),
      ))
      .push(Field::kv(
        "Remove default highlighter".blue(),
        bool::from(lang_config.remove_default_highlighter)
          .to_string()
          .green(),
      ));

    section
  }

  fn grammar_fields(&self, section: &mut Section, lang: &str, grammar_config: &GrammarConfig) {
    let grammar_install_path = self
      .resources
      .grammar_path_from_config(lang, grammar_config);
    let grammar_field = if let Ok(true) = grammar_install_path.try_exists() {
      Field::status_line(
        "",
        format!(
          "{} {}{}{}",
          "grammar".blue(),
          "(".black(),
          grammar_install_path.display().to_string().cyan(),
          ")".black()
        ),
      )
    } else if let Ok(true) = self.resources.grammars_dir(lang).try_exists() {
      Field::status_line(
        "󰈅",
        format!(
          "{lang} grammar out of sync; synchronize with {help}",
          help = format!("ktsctl sync {lang}").bold()
        ),
      )
    } else {
      Field::status_line(
        "",
        format!(
          "{lang} grammar missing; install with {help}",
          help = format!("ktsctl sync {lang}").bold()
        ),
      )
    };

    section.push(grammar_field);
  }

  fn queries_fields(&self, section: &mut Section, lang: &str, lang_config: &LanguageConfig) {
    let Some(queries_path) = self.resources.queries_dir_from_config(lang, lang_config) else {
      return;
    };

    if let Ok(true) = queries_path.try_exists() {
      let scm_files: HashSet<_> = queries_path
        .read_dir()
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|dir| dir.file_name().into_string())
        .collect();
      let mut scm_count = 0;
      let mut scm_expected_count = 0;
      let mut scm_field = |s: &str, desc: &str| {
        scm_expected_count += 1;

        if scm_files.contains(s) {
          scm_count += 1;
          let mut f = Field::status_line("", desc.blue());
          f.indent();
          f
        } else {
          let mut f = Field::status_line("", desc.blue());
          f.indent();
          f
        }
      };

      let fields = [
        scm_field("highlights.scm", "highlights"),
        scm_field("indents.scm", "indents"),
        scm_field("injections.scm", "injections"),
        scm_field("locals.scm", "locals"),
        scm_field("textobjects.scm", "text-objects"),
      ];

      if scm_count == scm_expected_count {
        section.push(Field::status_line(
          "",
          format!(
            "{} {}{}{}",
            "queries".blue(),
            "(".black(),
            queries_path.display().to_string().cyan(),
            ")".black()
          ),
        ));
      } else if scm_count > 0 {
        section.push(Field::status_line(
          "",
          format!(
            "{} {}{}{}",
            "queries".blue(),
            "(".black(),
            queries_path.display().to_string().cyan(),
            ")".black()
          ),
        ));
      } else {
        section.push(Field::status_line(
          "",
          format!(
            "{lang} queries missing; install with {help}",
            help = format!("ktsctl sync {lang}").bold()
          ),
        ));
      }

      section.extend(fields);
    } else {
      let queries_dir = self.resources.queries_dir(lang);

      let field = if let Ok(true) = queries_dir.try_exists() {
        Field::status_line(
          "",
          format!(
            "{lang} queries out of sync; synchronize with {help}",
            help = format!("ktsctl sync {lang}").bold()
          ),
        )
      } else {
        Field::status_line(
          "",
          format!(
            "{lang} queries missing; install with {help}",
            help = format!("ktsctl sync {lang}").bold()
          ),
        )
      };

      section.push(field);
    }
  }
}
