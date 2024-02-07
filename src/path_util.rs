use crate::util::*;
use std::path::PathBuf;

// Trait for extending std::path::PathBuf
use path_slash::PathBufExt as _;

pub fn to_slash(path: &PathBuf) -> Result<PathBuf> {
    match path.to_slash() {
        Some(slashed) => Ok(PathBuf::from(slashed.to_string())),
        _ => Err(format!(
            "Can't handle the path '{}'",
            path.to_str().unwrap_or("<not UTF-8 path>")
        )
        .into()),
    }
}

pub fn handle_path(input: Option<PathBuf>, working_dir: &PathBuf) -> Result<PathBuf> {
    if working_dir.is_relative() {
        return Err(format!(
            "Working directory is not absolute: {}",
            working_dir.to_str().unwrap_or("<not UTF-8 path>")
        )
        .into());
    }

    let path = match input {
        Some(path) => working_dir.join(path),
        _ => working_dir.clone(),
    };

    if !path.exists() {
        return Err(format!(
            "Path does not exist: {}",
            path.to_str().unwrap_or("<not UTF-8 path>")
        )
        .into());
    }

    to_slash(&path)
}
