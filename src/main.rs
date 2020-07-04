mod commands;

use clap::Clap;
use git2::{Cred, Error as GitError, IndexAddOption, Repository};
use std::env;
use std::io;
use std::io::prelude::*;
use std::path::PathBuf;

#[derive(Clap)]
#[clap(version = "0.1.0")]
struct Options {
    #[clap(subcommand)]
    cmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    Init,
    Add(AddOpts),
    Commit(CommitOpts),
    Remote(RemoteOpts),
    Push,
}

#[derive(Clap)]
struct AddOpts {
    files: Vec<String>,
}
#[derive(Clap)]
struct CommitOpts {
    msg: String,
}
#[derive(Clap)]
struct RemoteOpts {
    url: String,
}

fn get_cred(cred: &str) -> String {
    let mut input = String::new();
    print!("{}: ", cred);
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn get_home_dir() -> PathBuf {
    let home_dir = env::var("HOME").unwrap();
    PathBuf::from(&home_dir)
}

fn get_repo() -> Result<Repository, GitError> {
    let home_dir = get_home_dir();
    let repo_dir = home_dir.join(".config/bdm/repo.git");

    let repo = Repository::open(repo_dir)?;
    repo.set_workdir(&home_dir, false)?;
    Ok(repo)
}

fn main() -> Result<(), GitError> {
    let opts: Options = Options::parse();

    match opts.cmd {
        SubCommand::Init => {
            let home_dir = get_home_dir();
            let repo_dir = home_dir.join(".config/bdm/repo.git");

            let repo = Repository::init_bare(repo_dir)?;

            let sig = repo.signature()?;
            let tree_id = {
                let mut idx = repo.index()?;
                idx.write_tree()?
            };
            let tree = repo.find_tree(tree_id)?;

            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
        }
        SubCommand::Add(opts) => {
            let repo = get_repo()?;
            let mut index = repo.index()?;

            index.add_all(opts.files, IndexAddOption::DEFAULT, None)?;
            index.write()?;
        }
        SubCommand::Commit(opts) => {
            let repo = get_repo()?;

            let sig = repo.signature()?;
            let tree_id = {
                let mut idx = repo.index()?;
                idx.write_tree()?
            };
            let tree = repo.find_tree(tree_id)?;
            let old_head = repo.head()?.peel_to_commit()?;

            repo.commit(Some("HEAD"), &sig, &sig, &opts.msg, &tree, &[&old_head])?;
        }
        SubCommand::Remote(opts) => {
            let repo = get_repo()?;
            repo.remote("origin", &opts.url)?;
        }
        SubCommand::Push => {
            let repo = get_repo()?;
            let mut remote = repo.find_remote("origin")?;

            let mut username = None;
            let mut password = None;

            let mut cb = git2::RemoteCallbacks::new();
            let mut auth_cb =
                |_: &str, _: Option<&str>, _: git2::CredentialType| -> Result<Cred, GitError> {
                    if username.is_none() {
                        username = Some(get_cred("username"));
                    }
                    if password.is_none() {
                        password = Some(get_cred("password"));
                    }
                    Cred::userpass_plaintext(&username.clone().unwrap(), &password.clone().unwrap())
                };
            cb.credentials(&mut auth_cb);
            remote.connect_auth(git2::Direction::Push, Some(cb), None)?;

            let mut cb = git2::RemoteCallbacks::new();
            cb.credentials(&mut auth_cb);
            let mut push_opts = git2::PushOptions::new();
            push_opts.remote_callbacks(cb);

            remote.push(
                &["refs/heads/master:refs/heads/master"],
                Some(&mut push_opts),
            )?;
        }
    }

    Ok(())
}
