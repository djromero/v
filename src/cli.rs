use std::io;

#[derive(Debug, Eq, PartialEq)]
pub enum Arguments {
    Checkpoint(Option<String>),
    Undo,
    Redo,
    Squash(String),
    Discard,
    Help,
}

impl Arguments {
    pub fn parse<I>(arguments: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut arguments = arguments.into_iter();
        let Some(first) = arguments.next() else {
            return Ok(Self::Checkpoint(None));
        };
        match first.as_str() {
            "--undo" | "-u" | "--redo" | "-r" | "--discard" | "-d" | "--help" | "-h" => {
                if arguments.next().is_some() {
                    return Err(io::Error::other(format!("{first} accepts no arguments")));
                }
                Ok(match first.as_str() {
                    "--undo" | "-u" => Self::Undo,
                    "--redo" | "-r" => Self::Redo,
                    "--discard" | "-d" => Self::Discard,
                    _ => Self::Help,
                })
            }
            "--squash" | "-s" => {
                let message = arguments.collect::<Vec<_>>().join(" ");
                if message.trim().is_empty() || message.contains(['\n', '\r', '\0']) {
                    return Err(io::Error::other(
                        "--squash requires a nonblank, single-line message",
                    ));
                }
                if crate::version::is_version_subject(message.trim()) {
                    return Err(io::Error::other(
                        "squash message must not be a checkpoint subject",
                    ));
                }
                Ok(Self::Squash(message))
            }
            _ => {
                let words = if first == "--" {
                    Vec::new()
                } else {
                    vec![first]
                };
                let message = words
                    .into_iter()
                    .chain(arguments)
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Self::Checkpoint((!message.is_empty()).then_some(message)))
            }
        }
    }
}

pub const HELP: &str = "v: zero effort local versioning

Usage: v [message...]
       v --undo | --redo | --discard
       v --squash <message...>
       v --help
       v -- <literal checkpoint message...>

Run v to stage local changes and create a versioned Git commit (v0, v1, ...).
Optional message words become the commit body. Nothing is pushed.
With nothing to commit, v succeeds with only a blue checkmark.

Commands:
  -u, --undo              Step back one checkpoint and restore its files
  -r, --redo              Restore the next undone or discarded checkpoint
  -s, --squash <message>   Combine the current checkpoint run into one commit
  -d, --discard           Remove the current checkpoint run and restore its base
  -h, --help              Show this help

History commands require a clean index and working tree, including no untracked
non-ignored files. Stash, delete, or reset changes yourself before retrying.
Undo stops at the ordinary commit before the checkpoints. Redo follows one path.
A new commit or squash ends the old redo path. Squash cannot be undone with v.
";

#[cfg(test)]
mod tests {
    use super::Arguments;

    fn parse(words: &[&str]) -> std::io::Result<Arguments> {
        Arguments::parse(words.iter().map(|word| (*word).to_owned()))
    }

    #[test]
    fn checkpoint_messages_and_literal_flags() {
        assert_eq!(parse(&[]).unwrap(), Arguments::Checkpoint(None));
        assert_eq!(
            parse(&["experimental", "launcher"]).unwrap(),
            Arguments::Checkpoint(Some("experimental launcher".into()))
        );
        assert_eq!(
            parse(&["--", "--undo"]).unwrap(),
            Arguments::Checkpoint(Some("--undo".into()))
        );
        assert_eq!(
            parse(&["--", "--help"]).unwrap(),
            Arguments::Checkpoint(Some("--help".into()))
        );
    }

    #[test]
    fn history_arguments() {
        assert_eq!(parse(&["--undo"]).unwrap(), Arguments::Undo);
        assert_eq!(parse(&["--redo"]).unwrap(), Arguments::Redo);
        assert_eq!(parse(&["--discard"]).unwrap(), Arguments::Discard);
        assert_eq!(
            parse(&["--squash", "fix", "bug"]).unwrap(),
            Arguments::Squash("fix bug".into())
        );
        assert_eq!(
            parse(&["--squash", "v0..v1", "fix"]).unwrap(),
            Arguments::Squash("v0..v1 fix".into())
        );
        for words in [
            &["--undo", "extra"][..],
            &["--redo", "--undo"],
            &["--discard", "extra"],
            &["--squash"],
            &["--squash", " "],
            &["--squash", "v3"],
            &["--squash", "fix\nbug"],
        ] {
            assert!(parse(words).is_err(), "{words:?}");
        }
    }
    #[test]
    fn short_aliases_match_long_commands() {
        for (short, long) in [
            ("-u", "--undo"),
            ("-r", "--redo"),
            ("-d", "--discard"),
            ("-h", "--help"),
        ] {
            assert_eq!(parse(&[short]).unwrap(), parse(&[long]).unwrap());
            assert!(parse(&[short, "extra"]).is_err());
            assert!(parse(&[long, "extra"]).is_err());
            assert_eq!(
                parse(&["--", short]).unwrap(),
                Arguments::Checkpoint(Some(short.into()))
            );
        }
        assert_eq!(
            parse(&["-s", "fix", "bug"]).unwrap(),
            parse(&["--squash", "fix", "bug"]).unwrap()
        );
        for words in [&["-s"][..], &["-s", " "], &["-s", "v3"]] {
            assert!(parse(words).is_err());
        }
    }
}
