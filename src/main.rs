mod cli;
mod finish;
mod git;
mod history;
mod repository;
mod version;
mod workflow;

use std::io;
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus};

use repository::RepositoryState;

fn main() -> ExitCode {
    let arguments = cli::Arguments::parse(std::env::args().skip(1));
    if matches!(&arguments, Ok(cli::Arguments::Help)) {
        print!("{}", cli::HELP);
        return ExitCode::SUCCESS;
    }
    match arguments.and_then(run) {
        Ok(Some(subject)) => {
            println!("\x1b[32m✔\x1b[0m {subject}");
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!("\x1b[34m✔\x1b[0m");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("\x1b[31mｘ\x1b[0m{error}");
            error
                .get_ref()
                .and_then(|error| error.downcast_ref::<git::CommandError>())
                .map_or(ExitCode::FAILURE, |error| exit_code(error.status))
        }
    }
}

fn run(arguments: cli::Arguments) -> io::Result<Option<String>> {
    if let cli::Arguments::Done(message) = &arguments {
        return finish::done(message.as_deref()).map(Some);
    }
    if !matches!(arguments, cli::Arguments::Checkpoint(_)) {
        return history::run(&arguments).map(Some);
    }
    let cli::Arguments::Checkpoint(message) = arguments else {
        unreachable!()
    };
    let directory = std::env::current_dir()?;
    let home = home_directory()?;
    let git = git::Git::new(&directory);

    let (subject, committed) = match repository::discover(&directory, &home)? {
        RepositoryState::Unversioned => {
            let committed = workflow::bootstrap(&git, &directory, message.as_deref())?;
            ("v0".to_owned(), committed)
        }
        RepositoryState::Versioned => {
            let subject = version::next_subject(&git)?;
            let committed = workflow::checkpoint(&git, &directory, &subject, message.as_deref())?;
            (subject, committed)
        }
        RepositoryState::OrdinaryRoot => {
            let subject = version::next_subject_from_commit_count(&git)?;
            let committed = workflow::checkpoint(&git, &directory, &subject, message.as_deref())?;
            (subject, committed)
        }
    };
    Ok(committed.then_some(subject))
}

fn home_directory() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is not configured"))
}

fn exit_code(status: ExitStatus) -> ExitCode {
    status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map_or(ExitCode::FAILURE, ExitCode::from)
}
