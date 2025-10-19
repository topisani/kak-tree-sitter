//! Git utilities.

use std::{fs, path::Path};

use crate::{error::HellNo, process::Process, ui::report::Report};

/// Result of a successful git clone.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Clone {
  /// The repository was cloned remotely.
  Cloned,

  /// The repository was already cloned and thus a cached version is used.
  Cached,
}

/// Clone a git repository.
///
/// Return `Ok(true)` if something was cloned; `Ok(false)` if it was already there.
pub fn clone(report: Report, fetch_path: &Path, url: &str) -> Result<Clone, HellNo> {
  // ensure the path exists
  fs::create_dir_all(fetch_path).map_err(|err| HellNo::CannotCreateDir {
    dir: fetch_path.to_owned(),
    err,
  })?;

  // check whether the path has a .git in it; if not, clone
  let fetched;
  if let Ok(false) = fetch_path.join(".git").try_exists() {
    report!(report, "cloning {url}");

    // shallow clone of the repository
    let git_clone_args = [
      "clone",
      "--depth",
      "1",
      "-n",
      url,
      fetch_path.as_os_str().to_str().ok_or(HellNo::BadPath)?,
    ];

    Process::new("git").run(None, &git_clone_args)?;
    fetched = Clone::Cloned;
  } else {
    fetched = Clone::Cached;
  }

  Ok(fetched)
}

/// Checkout a source at a given pin.
pub fn checkout(report: Report, url: &str, fetch_path: &Path, pin: &str) -> Result<(), HellNo> {
  report!(
    report,
    "checking out {url} at {pin}",
    url = url.italic(),
    pin = pin.yellow()
  );
  let report = report.incr();

  Process::new("git").run(fetch_path, &["checkout", pin])?;

  report_success!(
    report,
    "checked out {url} at {pin}",
    url = url.italic(),
    pin = pin.yellow()
  );
  Ok(())
}

/// Fetch remote git objects.
///
/// This function expects a `pin` to prevent fetching the whole remote repository.
pub fn fetch(
  report: Report,
  lang: &str,
  fetch_path: &Path,
  url: &str,
  pin: &str,
) -> Result<(), HellNo> {
  report!(
    report,
    "fetching {lang} git remote objects {url}",
    lang = lang.blue(),
    url = url.italic()
  );
  let report = report.incr();

  Process::new("git").run(fetch_path, &["fetch", "origin", "--prune", pin])?;

  checkout(report.incr(), url, fetch_path, pin)?;

  report_success!(
    report,
    "fetched {lang} from {url} at {pin}",
    lang = lang.blue(),
    url = url.italic(),
    pin = pin.yellow()
  );
  Ok(())
}
