use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

pub struct Git {
    directory: PathBuf,
    index: Option<PathBuf>,
}

impl Git {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_owned(),
            index: None,
        }
    }

    pub fn with_index(directory: &Path, index: &Path) -> Self {
        Self {
            directory: directory.to_owned(),
            index: Some(index.to_owned()),
        }
    }

    pub fn run<I, S>(&self, arguments: I) -> io::Result<ExitStatus>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.output(arguments)?;
        if output.status.success() {
            Ok(output.status)
        } else {
            Err(command_error(output))
        }
    }

    pub fn output<I, S>(&self, arguments: I) -> io::Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command().args(arguments).output()
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new("git");
        command.current_dir(&self.directory);
        if let Some(index) = &self.index {
            command.env("GIT_INDEX_FILE", index);
        }
        command
    }

    pub fn text<I, S>(&self, arguments: I) -> io::Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.output(arguments)?;
        if !output.status.success() {
            return Err(command_error(output));
        }
        String::from_utf8(output.stdout)
            .map(|text| text.trim_end_matches('\n').to_owned())
            .map_err(io::Error::other)
    }

    pub fn commit(&self, subject: &str, body: Option<&str>) -> io::Result<bool> {
        let changes = self.output(["diff", "--cached", "--quiet", "--exit-code"])?;
        match changes.status.code() {
            Some(0) => Ok(false),
            Some(1) => {
                self.run(commit_arguments(subject, body))?;
                Ok(true)
            }
            _ => Err(command_error(changes)),
        }
    }

    pub fn is_ignored(&self, path: &OsStr) -> io::Result<bool> {
        self.output([
            OsStr::new("check-ignore"),
            OsStr::new("--quiet"),
            OsStr::new("--"),
            path,
        ])
        .and_then(|output| match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_error(output)),
        })
    }
}

#[derive(Debug)]
pub struct CommandError {
    pub status: ExitStatus,
    message: String,
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

pub fn command_error(output: Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if !stderr.trim().is_empty() {
        stderr.trim().to_owned()
    } else if !stdout.trim().is_empty() {
        stdout.trim().to_owned()
    } else {
        format!("git failed with {}", output.status)
    };
    io::Error::other(CommandError {
        status: output.status,
        message,
    })
}

fn commit_arguments(subject: &str, body: Option<&str>) -> Vec<OsString> {
    let mut arguments = vec!["commit".into(), "-m".into(), subject.into()];

    if let Some(body) = body {
        arguments.extend([OsString::from("-m"), OsString::from(body)]);
    }

    arguments
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::commit_arguments;

    #[test]
    fn constructs_a_subject_only_commit() {
        assert_eq!(
            commit_arguments("v7", None),
            os_strings(["commit", "-m", "v7"])
        );
    }

    #[test]
    fn constructs_the_user_message_as_a_separate_body() {
        assert_eq!(
            commit_arguments("v0", Some("experimental launcher")),
            os_strings(["commit", "-m", "v0", "-m", "experimental launcher"])
        );
    }

    fn os_strings<const N: usize>(values: [&str; N]) -> Vec<OsString> {
        values.into_iter().map(OsString::from).collect()
    }
}
