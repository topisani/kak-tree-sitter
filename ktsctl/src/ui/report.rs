//! Report macros.
//!
//! Use these macros to report information while operating with ktsctl.

use std::fmt::Display;

/// A report used for standardized formatting of ktsctl output commands.
///
/// Use the associated macros to use it.
#[derive(Clone, Copy, Debug)]
pub struct Report {
  /// Depth at which we are in the report. For instance, fetching > rust > grammar, fetching > rust > queries, etc.
  ///
  /// This is mainly used for indentation.
  depth: u8,
}

impl Report {
  pub fn new() -> Self {
    Self { depth: 0 }
  }

  /// Deepen the report by incrementing the amount of indentation.
  pub fn incr(self) -> Self {
    Self {
      depth: self.depth + 1,
    }
  }
}

impl Display for Report {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for _ in 0..self.depth {
      f.write_str("  ")?;
    }

    Ok(())
  }
}

macro_rules! report {
  ($report:expr, $($arg:tt)*) => {{
    use colored::Colorize as _;
    print!("{}{} ", $report, "•".black());
    println!($($arg)*)
  }}
}

macro_rules! report_success {
  ($report:expr, $($arg:tt)*) => {{
    use colored::Colorize as _;
    print!("{}{} ", $report, "".green());
    println!($($arg)*)
  }}
}

macro_rules! report_error {
  ($report:expr, $($arg:tt)*) => {{
    use colored::Colorize as _;
    print!("{}{} ", $report, "".red());
    println!($($arg)*)
  }}
}

macro_rules! report_info {
  ($report:expr, $($arg:tt)*) => {{
    use colored::Colorize as _;
    print!("{}{} ", $report, "󰈅".blue());
    println!($($arg)*)
  }}
}

macro_rules! report_warn {
  ($report:expr, $($arg:tt)*) => {{
    use colored::Colorize as _;
    print!("{}{} ", $report, "".yellow());
    println!($($arg)*)
  }}
}
