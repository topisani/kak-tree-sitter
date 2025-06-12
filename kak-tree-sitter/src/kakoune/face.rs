//! Face definition.

use std::fmt::Write;

use kak_tree_sitter_config::Config;

use crate::error::OhNo;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Face {
  name: String,
}

impl Face {
  /// Create a [`Face`] from a capture group; e.g. constant.character.escape.
  pub fn from_capture_group(name: impl AsRef<str>) -> Self {
    let name = name.as_ref().replace('.', "_");
    let name = format!("ts_{name}");
    Self { name }
  }

  pub fn to_kak(&self, f: &mut impl Write) -> Result<(), OhNo> {
    // compute the value; default for top-level names, and parent for the rest
    let value = self.name[3..]
      .rfind('_')
      .map(|last_delim_index| &self.name[..last_delim_index + 3])
      .unwrap_or("default");

    log::debug!("emitting face: {} {}", self.name, value);
    writeln!(f, "set-face global {} {}", self.name, value).map_err(|err| OhNo::FaceInit { err })
  }
}

/// Compute faces’ names and values.
///
/// Faces names are derived from the config syntax, which is "foo.bar.zoo.quux"; here, a face named
/// ts_foo_bar_zoo_quux will be linked to the face ts_foo_bar_zoo, which then must exist.
///
/// Faces that have a single element are linked to the default face by default.
pub fn compute_faces(config: &Config) -> Vec<Face> {
  let mut faces: Vec<_> = config
    .highlight
    .groups
    .iter()
    .map(Face::from_capture_group)
    .collect();
  faces.sort();
  faces
}

pub fn faces_to_kak(faces: &[Face]) -> String {
  let mut out = String::new();
  for face in faces {
    if let Err(err) = face.to_kak(&mut out) {
      log::error!("error while initializing {face:?}: {err}");
    }
  }

  out
}
