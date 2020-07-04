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
    Pull,
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

/// Get credentials form the user from `stdin`
fn get_cred(cred: &str) -> String {
    let mut input = String::new();
    print!("{}: ", cred);
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

/// Get the current user's home directory
fn get_home_dir() -> PathBuf {
    let home_dir = env::var("HOME").unwrap();
    PathBuf::from(&home_dir)
}

/// Open the default git repo
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

            // We have to make 2 sets of RemoteCallbacks for connecting to the remote and pushing
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

            let mut cb = git2::RemoteCallbacks::new();
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
        SubCommand::Pull => {
            let repo = get_repo()?;
            let mut remote = repo.find_remote("origin")?;

            remote.fetch(&["master"], None, None)?;

            let commit = repo.find_reference("FETCH_HEAD")?;
            let commit = repo.reference_to_annotated_commit(&commit)?;

            let (analysis, _) = repo.merge_analysis(&[&commit])?;

            let refname = "refs/heads/master";
            if analysis.is_fast_forward() {
                match repo.find_reference(refname) {
                    Ok(mut r) => {
                        let name = String::from_utf8_lossy(r.name_bytes()).to_string();
                        let msg = format!("Fast-Forward: Setting {} to id: {}", name, commit.id());
                        r.set_target(commit.id(), &msg)?;
                        repo.set_head(&name)?;
                        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
                    }
                    Err(_) => {
                        repo.reference(
                            &refname,
                            commit.id(),
                            true,
                            &format!("Setting master to {}", commit.id()),
                        )?;
                        repo.set_head(&refname)?;
                        repo.checkout_head(Some(
                            git2::build::CheckoutBuilder::default()
                                .allow_conflicts(true)
                                .conflict_style_merge(true)
                                .force(),
                        ))?;
                    }
                }
            } else if analysis.is_normal() {
                let head = repo.reference_to_annotated_commit(&repo.head()?)?;
                let local_tree = repo.find_commit(head.id())?.tree()?;
                let remote_tree = repo.find_commit(commit.id())?.tree()?;
                let ancestor = repo
                    .find_commit(repo.merge_base(head.id(), commit.id())?)?
                    .tree()?;
                let mut idx = repo.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

                if idx.has_conflicts() {
                    println!("Merge conficts detected...");
                    repo.checkout_index(Some(&mut idx), None)?;
                    return Ok(());
                }
                let result_tree = repo.find_tree(idx.write_tree_to(&repo)?)?;
                // now create the merge commit
                let msg = format!("Merge: {} into {}", commit.id(), head.id());
                let sig = repo.signature()?;
                let local_commit = repo.find_commit(head.id())?;
                let remote_commit = repo.find_commit(commit.id())?;
                // Do our merge commit and set current branch head to that commit.
                let _merge_commit = repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &msg,
                    &result_tree,
                    &[&local_commit, &remote_commit],
                )?;
                // Set working tree to match head.
                repo.checkout_head(None)?;
            } else {
                println!("nothing to do...")
            }
        }
    }

    Ok(())
}
