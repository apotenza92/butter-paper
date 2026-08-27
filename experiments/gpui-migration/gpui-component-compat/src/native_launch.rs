use std::{ffi::OsString, path::PathBuf};

use crate::document_workspace::{DocumentOpenBatchRequest, DocumentOpenOrigin};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeLaunchConfig {
    open_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLaunchError {
    MissingOpenPath,
    CurrentDirectoryUnavailable(String),
}

impl std::fmt::Display for NativeLaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOpenPath => formatter.write_str("--open requires a PDF path"),
            Self::CurrentDirectoryUnavailable(error) => {
                write!(
                    formatter,
                    "the launch working directory is unavailable: {error}"
                )
            }
        }
    }
}

impl std::error::Error for NativeLaunchError {}

impl NativeLaunchConfig {
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, NativeLaunchError> {
        let working_directory = std::env::current_dir()
            .map_err(|error| NativeLaunchError::CurrentDirectoryUnavailable(error.to_string()))?;
        Self::parse_in(arguments, working_directory)
    }

    pub fn parse_in(
        arguments: impl IntoIterator<Item = OsString>,
        working_directory: impl Into<PathBuf>,
    ) -> Result<Self, NativeLaunchError> {
        let mut arguments = arguments.into_iter().peekable();
        let working_directory = working_directory.into();
        let mut open_paths = Vec::new();
        while let Some(argument) = arguments.next() {
            if argument == "-ApplePersistenceIgnoreState" {
                if arguments.peek().is_some_and(|next| next == "YES") {
                    arguments.next();
                }
                continue;
            }
            let selected = if argument == "--open" {
                PathBuf::from(arguments.next().ok_or(NativeLaunchError::MissingOpenPath)?)
            } else if argument.as_encoded_bytes().starts_with(b"-") {
                continue;
            } else {
                PathBuf::from(argument)
            };
            if !selected
                .extension()
                .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"pdf"))
            {
                continue;
            }
            let selected = if selected.is_absolute() {
                selected
            } else {
                working_directory.join(selected)
            };
            if !open_paths.contains(&selected) {
                open_paths.push(selected);
            }
        }
        Ok(Self { open_paths })
    }

    pub fn open_paths(&self) -> &[PathBuf] {
        &self.open_paths
    }

    pub fn document_open_request(&self) -> DocumentOpenBatchRequest {
        DocumentOpenBatchRequest::new(DocumentOpenOrigin::System, self.open_paths.iter().cloned())
    }
}
