use crate::util::*;
use std::path::{Path, PathBuf};

// Trait for extending std::path::PathBuf
use path_slash::PathExt as _;

pub enum PathExists {
    MustExist,
    MustNotExist,
    DontCare,
}

pub fn to_slash(path: &Path) -> Result<PathBuf> {
    match path.to_slash() {
        Some(slashed) => Ok(PathBuf::from(slashed.into_owned())),
        _ => Err(format!("Can't handle the path '{}'", path.display()).into()),
    }
}

pub fn handle_path(
    input: Option<PathBuf>,
    working_dir: &Path,
    force_exists: Option<PathExists>,
) -> Result<PathBuf> {
    if working_dir.is_relative() {
        return Err(format!(
            "Working directory is not absolute: {}",
            working_dir.display()
        )
        .into());
    }

    let path = match input {
        Some(path) => &working_dir.join(path),
        _ => working_dir,
    };

    match force_exists.unwrap_or(PathExists::MustExist) {
        PathExists::MustExist => {
            if !path.exists() {
                return Err(format!("Path does not exist: {}", path.display()).into());
            }
        }
        PathExists::MustNotExist => {
            if path.exists() {
                return Err(format!("Path already exists: {}", path.display()).into());
            }
        }
        _ => {}
    }

    to_slash(path)
}
