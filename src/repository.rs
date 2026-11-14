use std::io;
use std::path::Path;

use crate::git::Git;

pub const CREATED_CONFIG_KEY: &str = "v.created";

#[derive(Debug, Eq, PartialEq)]
pub enum RepositoryState {
    Versioned,
    OrdinaryRoot,
    Unversioned,
}

pub fn discover(directory: &Path, home: &Path) -> io::Result<RepositoryState> {
    let mut candidate = Some(directory);

    while let Some(current) = candidate {
        if current.join(".git").exists() {
            // Stop at the nearest repository, even when a marked repository
            // exists further up the tree. Git would also use this repository.
            let output = Git::new(current).output([
                "config",
                "--local",
                "--bool",
                "--get",
                CREATED_CONFIG_KEY,
            ])?;
            if output.status.success() {
                return Ok(if output.stdout == b"true\n" {
                    RepositoryState::Versioned
                } else {
                    ordinary_repository_state(current, directory)
                });
            }
            if output.status.code() == Some(1) {
                return Ok(ordinary_repository_state(current, directory));
            }
            return Err(crate::git::command_error(output));
        }

        if current == home {
            break;
        }

        candidate = current.parent();
    }

    Ok(RepositoryState::Unversioned)
}

fn ordinary_repository_state(root: &Path, directory: &Path) -> RepositoryState {
    if root == directory {
        RepositoryState::OrdinaryRoot
    } else {
        RepositoryState::Unversioned
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{discover, RepositoryState, CREATED_CONFIG_KEY};
    use crate::git::Git;

    fn create_v_repository(directory: &std::path::Path) {
        let git = Git::new(directory);
        assert!(git.output(["init"]).unwrap().status.success());
        assert!(git
            .output(["config", "--local", CREATED_CONFIG_KEY, "true"])
            .unwrap()
            .status
            .success());
    }

    #[test]
    fn finds_a_repository_in_the_invocation_directory() {
        let filesystem = TestFilesystem::new();
        create_v_repository(&filesystem.directory);

        assert_eq!(
            discover(&filesystem.directory, &filesystem.home).unwrap(),
            RepositoryState::Versioned
        );
    }

    #[test]
    fn finds_a_repository_in_an_ancestor() {
        let filesystem = TestFilesystem::new();
        create_v_repository(&filesystem.home);

        assert_eq!(
            discover(&filesystem.directory, &filesystem.home).unwrap(),
            RepositoryState::Versioned
        );
    }

    #[test]
    fn includes_the_home_directory_in_the_search() {
        let filesystem = TestFilesystem::new();
        create_v_repository(&filesystem.home);

        assert_eq!(
            discover(&filesystem.home, &filesystem.home).unwrap(),
            RepositoryState::Versioned
        );
    }

    #[test]
    fn does_not_search_above_the_home_directory() {
        let root = TempDir::new().unwrap();
        let home = root.path().join("home");
        let directory = home.join("project");
        fs::create_dir_all(&directory).unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();

        assert_eq!(
            discover(&directory, &home).unwrap(),
            RepositoryState::Unversioned
        );
    }

    #[test]
    fn reports_an_absent_repository() {
        let filesystem = TestFilesystem::new();

        assert_eq!(
            discover(&filesystem.directory, &filesystem.home).unwrap(),
            RepositoryState::Unversioned
        );
    }

    struct TestFilesystem {
        _root: TempDir,
        home: std::path::PathBuf,
        directory: std::path::PathBuf,
    }

    impl TestFilesystem {
        fn new() -> Self {
            let root = TempDir::new().unwrap();
            let home = root.path().join("home");
            let directory = home.join("project").join("nested");
            fs::create_dir_all(&directory).unwrap();

            Self {
                _root: root,
                home,
                directory,
            }
        }
    }
}
