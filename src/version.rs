use std::io;

use crate::git::Git;

pub fn next_subject(git: &Git) -> io::Result<String> {
    let output = git.output(["log", "-1", "--format=%s"])?;
    if !output.status.success() {
        return Err(crate::git::command_error(output));
    }

    let subject = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let subject = subject.trim_end_matches(['\r', '\n']);

    if is_version_subject(subject) {
        increment_subject(subject)
    } else {
        next_subject_from_commit_count(git)
    }
}

pub fn next_subject_from_commit_count(git: &Git) -> io::Result<String> {
    let output = git.output(["rev-list", "--count", "HEAD"])?;
    if !output.status.success() {
        return Err(crate::git::command_error(output));
    }

    let count = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let number = count
        .trim_end_matches(['\r', '\n'])
        .parse::<u64>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "commit count overflow"))?;

    Ok(format!("v{number}"))
}

pub fn is_version_subject(subject: &str) -> bool {
    subject.strip_prefix('v').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn increment_subject(subject: &str) -> io::Result<String> {
    let number = subject
        .strip_prefix('v')
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "version subject has no v prefix",
            )
        })?
        .parse::<u64>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let number = number
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "version number overflow"))?;

    Ok(format!("v{number}"))
}

#[cfg(test)]
mod tests {
    use super::{increment_subject, is_version_subject};

    #[test]
    fn increments_the_first_version() {
        assert_eq!(increment_subject("v0").unwrap(), "v1");
    }

    #[test]
    fn increments_a_multi_digit_version() {
        assert_eq!(increment_subject("v19").unwrap(), "v20");
    }

    #[test]
    fn recognizes_only_v_followed_by_digits_as_a_version_subject() {
        assert!(is_version_subject("v19"));
        assert!(!is_version_subject("release 19"));
        assert!(!is_version_subject("v"));
        assert!(!is_version_subject("v1-beta"));
    }
}
