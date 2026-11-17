use std::{
    hash::Hash,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use chrono::{DateTime, FixedOffset};
use rustc_hash::FxHashMap;

use crate::Result;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitHash(String);

impl CommitHash {
    pub fn as_short_hash(&self) -> &str {
        &self.0[0..7]
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CommitHash {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug, Default, Clone)]
pub struct Commit {
    pub commit_hash: CommitHash,
    pub author_name: String,
    pub author_email: String,
    pub author_date: DateTime<FixedOffset>,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_date: DateTime<FixedOffset>,
    pub subject: String,
    pub body: String,
    pub parent_commit_hashes: Vec<CommitHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ref {
    Tag {
        name: String,
        target: CommitHash,
    },
    Branch {
        name: String,
        target: CommitHash,
    },
    RemoteBranch {
        name: String,
        target: CommitHash,
    },
    Stash {
        name: String,
        message: String,
        target: CommitHash,
    },
}

impl Ref {
    pub fn name(&self) -> &str {
        match self {
            Ref::Tag { name, .. } => name,
            Ref::Branch { name, .. } => name,
            Ref::RemoteBranch { name, .. } => name,
            Ref::Stash { name, .. } => name,
        }
    }

    pub fn target(&self) -> &CommitHash {
        match self {
            Ref::Tag { target, .. } => target,
            Ref::Branch { target, .. } => target,
            Ref::RemoteBranch { target, .. } => target,
            Ref::Stash { target, .. } => target,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Head {
    Branch { name: String },
    Detached { target: CommitHash },
    None,
}

#[derive(Debug, Clone, Copy)]
pub enum SortCommit {
    Chronological,
    Topological,
}

type CommitMap = FxHashMap<CommitHash, Commit>;
type CommitsMap = FxHashMap<CommitHash, Vec<CommitHash>>;

type RefMap = FxHashMap<CommitHash, Vec<Ref>>;

#[derive(Debug)]
pub struct Repository {
    path: PathBuf,
    commit_map: CommitMap,

    parents_map: CommitsMap,
    children_map: CommitsMap,

    ref_map: RefMap,
    head: Head,
    // to preserve order of the original commits from `git log`, we store the commit hashes
    commit_hashes: Vec<CommitHash>,

    // local branches checked out in a worktree other than this one, with that worktree's path
    other_worktree_branches: FxHashMap<String, PathBuf>,
}

impl Repository {
    pub fn load(
        path: &Path,
        sort: SortCommit,
        max_count: Option<usize>,
        mailmap: bool,
    ) -> Result<Self> {
        check_git_repository(path)?;

        let (mut ref_map, head) = load_refs(path);

        let stashes = load_all_stashes(path, mailmap);
        let commits = load_all_commits(path, sort, &head, &stashes, max_count, mailmap);
        if commits.is_empty() {
            return Err("no commits in the repository".into());
        }

        let commits = merge_stashes_to_commits(commits, stashes);
        let commit_hashes = commits.iter().map(|c| c.commit_hash.clone()).collect();

        let (parents_map, children_map) = build_commits_maps(&commits);
        let commit_map = to_commit_map(commits);

        let stash_ref_map = load_stashes_as_refs(path);
        merge_ref_maps(&mut ref_map, stash_ref_map);

        let other_worktree_branches = other_worktree_branches(load_worktrees(path), &head);

        let mut repository = Self::new(
            path.to_path_buf(),
            commit_map,
            parents_map,
            children_map,
            ref_map,
            head,
            commit_hashes,
        );
        repository.other_worktree_branches = other_worktree_branches;
        Ok(repository)
    }

    pub fn new(
        path: PathBuf,
        commit_map: CommitMap,
        parents_map: CommitsMap,
        children_map: CommitsMap,
        ref_map: RefMap,
        head: Head,
        commit_hashes: Vec<CommitHash>,
    ) -> Self {
        Self {
            path,
            commit_map,
            parents_map,
            children_map,
            ref_map,
            head,
            commit_hashes,
            other_worktree_branches: FxHashMap::default(),
        }
    }

    /// Re-read the worktree list so newly created agent worktrees appear immediately.
    pub fn worktrees(&self) -> std::result::Result<Vec<Worktree>, String> {
        run_git(&self.path, &["worktree", "list", "--porcelain", "-z"])
            .map(|output| parse_worktrees(&output))
    }

    /// Fresh lookup: an agent may have added a worktree since the graph was loaded.
    pub fn other_worktree_for_branch(
        &self,
        name: &str,
    ) -> std::result::Result<Option<PathBuf>, String> {
        let current = self.worktree_root()?;
        let current = std::fs::canonicalize(&current).unwrap_or(current);
        Ok(self.worktrees()?.into_iter().find_map(|worktree| {
            if worktree.bare || worktree.branch.as_deref() != Some(name) {
                return None;
            }
            let path = std::fs::canonicalize(&worktree.path).unwrap_or(worktree.path);
            (path != current).then_some(path)
        }))
    }

    pub fn worktree_root(&self) -> std::result::Result<PathBuf, String> {
        run_git(&self.path, &["rev-parse", "--show-toplevel"])
            .map(|output| PathBuf::from(output.strip_suffix('\n').unwrap_or(&output)))
    }

    /// Local branches checked out in a worktree other than this one.
    pub fn other_worktree_branches(&self) -> &FxHashMap<String, PathBuf> {
        &self.other_worktree_branches
    }

    /// Whether tracked files in this worktree have staged or unstaged changes.
    pub fn has_uncommitted_changes(&self) -> std::result::Result<bool, String> {
        let args = ["status", "--porcelain", "--untracked-files=no"];
        run_git(&self.path, &args).map(|stdout| !stdout.trim().is_empty())
    }

    /// Switches this worktree to a branch. Never forces, stashes or discards:
    /// whatever git refuses is returned as the error.
    pub fn switch_branch(&self, target: &BranchTarget) -> std::result::Result<(), String> {
        let result = match target {
            BranchTarget::Local(name) => run_git(&self.path, &["switch", name]),
            BranchTarget::Remote(name) => run_git(&self.path, &["switch", "--track", name]),
        };
        result.map(|_| ())
    }

    pub fn check_branch_deletion(&self, target: &BranchTarget) -> std::result::Result<(), String> {
        self.prepare_deletion(target).map(|_| ())
    }

    /// Re-read worktrees before every deletion, including force retries.
    fn prepare_deletion(
        &self,
        target: &BranchTarget,
    ) -> std::result::Result<PreparedDeletion, String> {
        let remote = match target {
            BranchTarget::Local(_) => None,
            BranchTarget::Remote(name) => Some(self.remote_branch_parts(name)?),
        };
        // The local branch with the target's own name, even if it tracks elsewhere.
        let same_name = remote
            .as_ref()
            .map_or_else(|| target.name().to_string(), |(_, branch)| branch.clone());
        // Local branches whose upstream is the remote branch being deleted.
        let mut tracking = Vec::new();
        if let BranchTarget::Remote(name) = target {
            let upstreams = run_git(
                &self.path,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(upstream)",
                    "refs/heads/",
                ],
            )?;
            let reference = format!("refs/remotes/{name}");
            for line in upstreams.lines() {
                if let Some((local, upstream)) = line.split_once('\0') {
                    if upstream == reference {
                        if let Some(local) = local.strip_prefix("refs/heads/") {
                            tracking.push(local.to_string());
                        }
                    }
                }
            }
        }
        let worktrees = parse_worktrees(&run_git(
            &self.path,
            &["worktree", "list", "--porcelain", "-z"],
        )?);
        for worktree in &worktrees {
            let Some(branch) = &worktree.branch else {
                continue;
            };
            let path = worktree.path.display();
            if tracking.contains(branch) {
                return Err(format!(
                    "'{branch}' tracks '{}' and is checked out in worktree {path}; not deleting",
                    target.name()
                ));
            }
            if *branch == same_name {
                return Err(format!(
                    "'{branch}' is checked out in worktree {path}; not deleting"
                ));
            }
        }
        let expected = self.deletion_ref(target)?;
        Ok(PreparedDeletion { expected, remote })
    }

    // Refuse stale selections: an agent may have moved the branch since the last refresh.
    fn deletion_ref(&self, target: &BranchTarget) -> std::result::Result<String, String> {
        let reference = match target {
            BranchTarget::Local(name) => format!("refs/heads/{name}"),
            BranchTarget::Remote(name) => format!("refs/remotes/{name}"),
        };
        let expected = self
            .all_refs()
            .into_iter()
            .find_map(|r| match (target, r) {
                (
                    BranchTarget::Local(name),
                    Ref::Branch {
                        name: actual,
                        target,
                    },
                )
                | (
                    BranchTarget::Remote(name),
                    Ref::RemoteBranch {
                        name: actual,
                        target,
                    },
                ) if name == actual => Some(target.as_str()),
                _ => None,
            })
            .ok_or_else(|| "Branch no longer exists; refresh first".to_string())?;
        let actual = run_git(&self.path, &["rev-parse", "--verify", &reference])?;
        if actual.trim() != expected {
            return Err("Branch changed since the last refresh; refresh before deleting".into());
        }
        Ok(expected.into())
    }

    fn remote_branch_parts(&self, name: &str) -> std::result::Result<(String, String), String> {
        let remotes = run_git(&self.path, &["remote"])?;
        let remote = remotes
            .lines()
            .filter(|r| name.starts_with(&format!("{r}/")))
            .max_by_key(|r| r.len())
            .ok_or_else(|| format!("No configured remote for '{name}'"))?;
        let branch = &name[remote.len() + 1..];
        if branch == "HEAD" || branch.is_empty() {
            return Err("Remote HEAD aliases cannot be deleted".into());
        }
        Ok((remote.into(), branch.into()))
    }

    /// Safe local delete by default. Remote deletion uses a lease so a newer
    /// remote tip cannot be removed using a stale remote-tracking reference.
    pub fn delete_branch(
        &self,
        target: &BranchTarget,
        force: bool,
    ) -> std::result::Result<(), DeleteBranchError> {
        if force && matches!(target, BranchTarget::Remote(_)) {
            return Err(DeleteBranchError::Other(
                "Remote force deletion is not supported".into(),
            ));
        }
        let PreparedDeletion { expected, remote } = self
            .prepare_deletion(target)
            .map_err(DeleteBranchError::Other)?;
        match (target, remote) {
            (BranchTarget::Local(name), _) => {
                let mode = if force { "-D" } else { "-d" };
                // The C locale keeps Git's "not fully merged" message matchable.
                run_git_with(&self.path, &["branch", mode, "--", name], |command| {
                    command.env("LC_ALL", "C");
                })
                .map(|_| ())
                .map_err(|error| {
                    if !force && error.contains("not fully merged") {
                        DeleteBranchError::NotMerged(error)
                    } else {
                        DeleteBranchError::Other(error)
                    }
                })
            }
            (BranchTarget::Remote(_), remote) => {
                let (remote, branch) = remote.expect("remote targets are split into parts");
                let reference = format!("refs/heads/{branch}");
                let lease = format!("--force-with-lease={reference}:{expected}");
                run_git_with(
                    &self.path,
                    &["push", &lease, "--delete", "--", &remote, &reference],
                    without_prompts,
                )
                .map(|_| ())
                .map_err(DeleteBranchError::Other)
            }
        }
    }

    pub fn commit(&self, commit_hash: &CommitHash) -> Option<&Commit> {
        self.commit_map.get(commit_hash)
    }

    pub fn all_commits(&self) -> Vec<&Commit> {
        self.commit_hashes
            .iter()
            .filter_map(|hash| self.commit(hash))
            .collect()
    }

    pub fn parents_hash(&self, commit_hash: &CommitHash) -> Vec<&CommitHash> {
        self.parents_map
            .get(commit_hash)
            .map(|hs| hs.iter().collect::<Vec<&CommitHash>>())
            .unwrap_or_default()
    }

    pub fn children_hash(&self, commit_hash: &CommitHash) -> Vec<&CommitHash> {
        self.children_map
            .get(commit_hash)
            .map(|hs| hs.iter().collect::<Vec<&CommitHash>>())
            .unwrap_or_default()
    }

    pub fn refs(&self, commit_hash: &CommitHash) -> Vec<&Ref> {
        self.ref_map
            .get(commit_hash)
            .map(|refs| refs.iter().collect::<Vec<&Ref>>())
            .unwrap_or_default()
    }

    pub fn all_refs(&self) -> Vec<&Ref> {
        self.ref_map.values().flatten().collect()
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    /// The branches pointing at a commit, sorted into what can be switched to.
    pub fn branches_at(&self, commit_hash: &CommitHash) -> BranchesAtCommit {
        let local_branches: Vec<&str> = self
            .all_refs()
            .into_iter()
            .filter_map(|r| match r {
                Ref::Branch { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let current = match &self.head {
            Head::Branch { name } => Some(name.as_str()),
            Head::Detached { .. } | Head::None => None,
        };
        branches_at_commit(&self.refs(commit_hash), &local_branches, current)
    }

    /// Include local and remote branches as distinct deletion choices.
    pub fn branches_for_deletion(&self, commit_hash: &CommitHash) -> Vec<BranchTarget> {
        let refs = self.refs(commit_hash);
        let mut locals = Vec::new();
        let mut remotes = Vec::new();
        for r in refs {
            match r {
                Ref::Branch { name, .. } => locals.push(name.clone()),
                Ref::RemoteBranch { name, .. } if !name.ends_with("/HEAD") => {
                    remotes.push(name.clone())
                }
                _ => {}
            }
        }
        locals.sort_unstable();
        remotes.sort_unstable();
        locals
            .into_iter()
            .map(BranchTarget::Local)
            .chain(remotes.into_iter().map(BranchTarget::Remote))
            .collect()
    }

    /// Prefer local branch names for copying; fall back to remote-tracking names.
    /// Unlike switching, copying includes the current branch and occupied branches.
    pub fn branch_names_for_copy(&self, commit_hash: &CommitHash) -> Vec<String> {
        branch_names_for_copy(&self.refs(commit_hash))
    }

    pub fn commit_detail(&self, commit_hash: &CommitHash) -> (Commit, Vec<FileChange>) {
        let commit = self.commit(commit_hash).unwrap().clone();
        let changes = if commit.parent_commit_hashes.is_empty() {
            get_initial_commit_additions(&self.path, commit_hash)
        } else {
            get_diff_summary(&self.path, commit_hash)
        };
        (commit, changes)
    }
}

fn branch_names_for_copy(refs: &[&Ref]) -> Vec<String> {
    let locals: Vec<String> = refs
        .iter()
        .filter_map(|r| match r {
            Ref::Branch { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let mut names = if locals.is_empty() {
        refs.iter()
            .filter_map(|r| match r {
                Ref::RemoteBranch { name, .. } if !name.ends_with("/HEAD") => Some(name.clone()),
                _ => None,
            })
            .collect()
    } else {
        locals
    };
    names.sort_unstable();
    names.dedup();
    names
}

struct PreparedDeletion {
    /// The branch tip loaded at the last refresh, verified against the repository.
    expected: String,
    /// Remote name and branch name, for remote targets.
    remote: Option<(String, String)>,
}

#[derive(Debug)]
pub enum DeleteBranchError {
    NotMerged(String),
    Other(String),
}

/// A branch the user asked to switch to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchTarget {
    /// A local branch, by its short name.
    Local(String),
    /// A remote-tracking branch such as `origin/topic`; switching to it creates
    /// a local branch that tracks it.
    Remote(String),
}

impl BranchTarget {
    pub fn name(&self) -> &str {
        match self {
            BranchTarget::Local(name) | BranchTarget::Remote(name) => name,
        }
    }
}

/// The branches pointing at one commit, from the point of view of switching.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BranchesAtCommit {
    /// Branches that can be switched to: local ones first, then remote ones
    /// that have no local branch yet.
    pub targets: Vec<BranchTarget>,
    /// The current branch points at this commit.
    pub has_current: bool,
    /// Remote branches left out because their local branch already exists at
    /// another commit, as (remote name, local name).
    pub shadowed_remotes: Vec<(String, String)>,
}

fn branches_at_commit(
    refs_at_commit: &[&Ref],
    local_branches: &[&str],
    current: Option<&str>,
) -> BranchesAtCommit {
    let mut locals: Vec<&str> = Vec::new();
    let mut remotes: Vec<&str> = Vec::new();
    for r in refs_at_commit {
        match r {
            Ref::Branch { name, .. } => locals.push(name),
            // `origin/HEAD` is a pointer to the remote's default branch, not a branch.
            Ref::RemoteBranch { name, .. } if !name.ends_with("/HEAD") => remotes.push(name),
            _ => {}
        }
    }
    locals.sort_unstable();
    remotes.sort_unstable();

    let mut result = BranchesAtCommit {
        has_current: current.is_some_and(|c| locals.contains(&c)),
        ..Default::default()
    };
    for &local in &locals {
        if Some(local) != current {
            result.targets.push(BranchTarget::Local(local.into()));
        }
    }
    for remote in remotes {
        // `git switch --track origin/topic` creates the local branch `topic`.
        let Some((_, local)) = remote.split_once('/') else {
            continue;
        };
        if locals.contains(&local) {
            // The local branch is here too and already covers it.
        } else if local_branches.contains(&local) {
            result.shadowed_remotes.push((remote.into(), local.into()));
        } else {
            result.targets.push(BranchTarget::Remote(remote.into()));
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub bare: bool,
}

fn load_worktrees(path: &Path) -> Vec<Worktree> {
    run_git(path, &["worktree", "list", "--porcelain", "-z"])
        .map(|stdout| parse_worktrees(&stdout))
        .unwrap_or_default()
}

fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut worktrees: Vec<Worktree> = Vec::new();
    // -z avoids Git's quoting of paths containing tabs, newlines or backslashes.
    let separator = if porcelain.contains('\0') { '\0' } else { '\n' };
    for line in porcelain.split(separator) {
        if let Some(path) = line.strip_prefix("worktree ") {
            worktrees.push(Worktree {
                path: PathBuf::from(path),
                branch: None,
                bare: false,
            });
        } else if line == "bare" {
            if let Some(worktree) = worktrees.last_mut() {
                worktree.bare = true;
            }
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(worktree) = worktrees.last_mut() {
                worktree.branch = Some(branch.into());
            }
        }
    }
    worktrees
}

// A branch can be checked out in only one worktree, so the worktree holding the
// current branch is this one and every other branch belongs to another worktree.
fn other_worktree_branches(worktrees: Vec<Worktree>, head: &Head) -> FxHashMap<String, PathBuf> {
    let current = match head {
        Head::Branch { name } => Some(name.as_str()),
        Head::Detached { .. } | Head::None => None,
    };
    worktrees
        .into_iter()
        .filter_map(|worktree| Some((worktree.branch?, worktree.path)))
        .filter(|(branch, _)| Some(branch.as_str()) != current)
        .collect()
}

// The number of graph lanes shown is remembered per repository in its git config,
// which every worktree of the repository shares.
const GRAPH_LANES_CONFIG_KEY: &str = "serie.graphLanes";

/// The number of graph lanes to show, as saved by an earlier run.
pub fn saved_graph_lanes(path: &Path) -> Option<usize> {
    let value = run_git(path, &["config", "--get", GRAPH_LANES_CONFIG_KEY]).ok()?;
    value.trim().parse().ok().filter(|lanes| *lanes > 0)
}

/// Remembers the number of graph lanes to show; `None` forgets it, so that
/// every lane is shown again.
pub fn save_graph_lanes(path: &Path, lanes: Option<usize>) -> std::result::Result<(), String> {
    let key = GRAPH_LANES_CONFIG_KEY;
    match lanes {
        Some(lanes) => run_git(path, &["config", "--local", key, &lanes.to_string()]).map(|_| ()),
        None => {
            // Unsetting a key that is not set is an error for git.
            if run_git(path, &["config", "--local", "--get", key]).is_err() {
                return Ok(());
            }
            run_git(path, &["config", "--local", "--unset", key]).map(|_| ())
        }
    }
}

fn run_git(path: &Path, args: &[&str]) -> std::result::Result<String, String> {
    run_git_with(path, args, |_| {})
}

fn run_git_with(
    path: &Path,
    args: &[&str],
    configure: impl FnOnce(&mut Command),
) -> std::result::Result<String, String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(path).stdin(Stdio::null());
    configure(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(git_error_message(
            &String::from_utf8_lossy(&output.stderr),
            args,
        ))
    }
}

// Git errors can span several lines, but they are shown on the one-line status bar.
/// Network commands must fail instead of prompting: Serie owns the terminal, so a
/// credential, passphrase or host-key prompt would draw over the UI and block it.
fn without_prompts(command: &mut Command) {
    command.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // A new session has no controlling terminal, so SSH cannot open /dev/tty.
        // Agents, keychains and credential helpers keep working.
        // SAFETY: setsid is async-signal-safe and touches no parent state.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
}

fn git_error_message(stderr: &str, args: &[&str]) -> String {
    let message = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    let message = message
        .strip_prefix("fatal: ")
        .or_else(|| message.strip_prefix("error: "))
        .unwrap_or(&message);
    if message.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        message.to_string()
    }
}

fn check_git_repository(path: &Path) -> Result<()> {
    if !is_inside_work_tree(path) && !is_bare_repository(path) {
        let msg = "not a git repository (or any of the parent directories)";
        return Err(msg.into());
    }
    Ok(())
}

fn is_inside_work_tree(path: &Path) -> bool {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(path)
        .output();
    output.is_ok_and(|output| output.status.success() && output.stdout == b"true\n")
}

fn is_bare_repository(path: &Path) -> bool {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--is-bare-repository")
        .current_dir(path)
        .output();
    output.is_ok_and(|output| output.status.success() && output.stdout == b"true\n")
}

fn load_all_commits(
    path: &Path,
    sort: SortCommit,
    head: &Head,
    stashes: &[Commit],
    max_count: Option<usize>,
    mailmap: bool,
) -> Vec<Commit> {
    let mut cmd = Command::new("git");
    cmd.arg("log");

    cmd.arg(match sort {
        SortCommit::Chronological => "--date-order",
        SortCommit::Topological => "--topo-order",
    })
    .arg(format!("--pretty={}", load_commits_format(mailmap)))
    .arg("--date=iso-strict")
    .arg("-z"); // use NUL as a delimiter

    // exclude stashes and other refs
    cmd.arg("--branches").arg("--remotes").arg("--tags");

    // commits that are reachable from the stashes
    stashes.iter().for_each(|stash| {
        cmd.arg(stash.parent_commit_hashes[0].as_str());
    });

    if !matches!(head, Head::None) {
        cmd.arg("HEAD");
    }

    if let Some(n) = max_count {
        cmd.arg("--max-count").arg(n.to_string());
    }

    cmd.current_dir(path).stdout(Stdio::piped());

    let mut process = cmd.spawn().unwrap();

    let stdout = process.stdout.take().expect("failed to open stdout");

    let reader = BufReader::new(stdout);

    let mut commits = Vec::new();

    for bytes in reader.split(b'\0') {
        let bytes = bytes.unwrap();
        let s = String::from_utf8_lossy(&bytes);

        let parts: Vec<&str> = s.split('\x1f').collect();
        if parts.len() != 10 {
            panic!("unexpected number of parts: {} [{}]", parts.len(), s);
        }

        let commit = Commit {
            commit_hash: parts[0].into(),
            author_name: parts[1].into(),
            author_email: parts[2].into(),
            author_date: parse_iso_date(parts[3]),
            committer_name: parts[4].into(),
            committer_email: parts[5].into(),
            committer_date: parse_iso_date(parts[6]),
            subject: parts[7].into(),
            body: parts[8].into(),
            parent_commit_hashes: parse_parent_commit_hashes(parts[9]),
        };

        commits.push(commit);
    }

    process.wait().unwrap();

    commits
}

fn load_all_stashes(path: &Path, mailmap: bool) -> Vec<Commit> {
    let mut cmd = Command::new("git")
        .arg("stash")
        .arg("list")
        .arg(format!("--pretty={}", load_commits_format(mailmap)))
        .arg("--date=iso-strict")
        .arg("-z") // use NUL as a delimiter
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");

    let reader = BufReader::new(stdout);

    let mut commits = Vec::new();

    for bytes in reader.split(b'\0') {
        let bytes = bytes.unwrap();
        let s = String::from_utf8_lossy(&bytes);

        let parts: Vec<&str> = s.split('\x1f').collect();
        if parts.len() != 10 {
            panic!("unexpected number of parts: {} [{}]", parts.len(), s);
        }

        let commit = Commit {
            commit_hash: parts[0].into(),
            author_name: parts[1].into(),
            author_email: parts[2].into(),
            author_date: parse_iso_date(parts[3]),
            committer_name: parts[4].into(),
            committer_email: parts[5].into(),
            committer_date: parse_iso_date(parts[6]),
            subject: parts[7].into(),
            body: parts[8].into(),
            parent_commit_hashes: parse_parent_commit_hashes(parts[9]),
        };

        commits.push(commit);
    }

    cmd.wait().unwrap();

    commits
}

fn load_commits_format(mailmap: bool) -> String {
    // The uppercase name/email placeholders (`%aN`, `%aE`, `%cN`, `%cE`) resolve
    // identities through the repository's .mailmap, while the lowercase variants
    // use the raw values recorded in each commit.
    let format = if mailmap {
        [
            "%H", "%aN", "%aE", "%ad", "%cN", "%cE", "%cd", "%s", "%b", "%P",
        ]
    } else {
        [
            "%H", "%an", "%ae", "%ad", "%cn", "%ce", "%cd", "%s", "%b", "%P",
        ]
    };
    format.join("%x1f") // use Unit Separator as a delimiter
}

fn parse_iso_date(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).unwrap()
}

fn parse_parent_commit_hashes(s: &str) -> Vec<CommitHash> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(' ').map(|s| s.into()).collect()
}

fn build_commits_maps(commits: &Vec<Commit>) -> (CommitsMap, CommitsMap) {
    let mut parents_map: CommitsMap = FxHashMap::default();
    let mut children_map: CommitsMap = FxHashMap::default();
    for commit in commits {
        let hash = &commit.commit_hash;
        for parent_hash in &commit.parent_commit_hashes {
            parents_map
                .entry(hash.clone())
                .or_default()
                .push(parent_hash.clone());
            children_map
                .entry(parent_hash.clone())
                .or_default()
                .push(hash.clone());
        }
    }

    (parents_map, children_map)
}

fn to_commit_map(commits: Vec<Commit>) -> CommitMap {
    commits
        .into_iter()
        .map(|commit| (commit.commit_hash.clone(), commit))
        .collect()
}

fn merge_stashes_to_commits(commits: Vec<Commit>, stashes: Vec<Commit>) -> Vec<Commit> {
    // Stash commit has multiple parent commits, but the first parent commit is the commit that the stash was created from.
    // If the first parent commit is not found, the stash commit is ignored.
    let mut ret = Vec::new();
    let mut statsh_map: FxHashMap<CommitHash, Vec<Commit>> =
        stashes
            .into_iter()
            .fold(FxHashMap::default(), |mut acc, commit| {
                let parent = commit.parent_commit_hashes[0].clone();
                acc.entry(parent).or_default().push(commit);
                acc
            });
    for commit in commits {
        if let Some(stashes) = statsh_map.remove(&commit.commit_hash) {
            for stash in stashes {
                ret.push(stash);
            }
        }
        ret.push(commit);
    }
    ret
}

fn load_refs(path: &Path) -> (RefMap, Head) {
    let mut cmd = Command::new("git")
        .arg("show-ref")
        .arg("--head")
        .arg("--dereference")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");

    let reader = BufReader::new(stdout);

    let mut ref_map = RefMap::default();
    let mut tag_map: FxHashMap<String, Ref> = FxHashMap::default();
    let mut head: Head = Head::None;

    for line in reader.lines() {
        let line = line.unwrap();

        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 2 {
            panic!("unexpected number of parts: {} [{}]", parts.len(), line);
        }

        let hash = parts[0];
        let refs = parts[1];

        if refs == "HEAD" {
            head = if let Some(branch) = get_current_branch(path) {
                Head::Branch { name: branch }
            } else {
                Head::Detached {
                    target: hash.into(),
                }
            };
        } else if let Some(r) = parse_branch_refs(hash, refs) {
            ref_map.entry(hash.into()).or_default().push(r);
        } else if let Some(r) = parse_tag_refs(hash, refs) {
            // if annotated tag exists, it will be overwritten by the following line of the same tag
            // this will make the tag point to the commit that the annotated tag points to
            tag_map.insert(r.name().into(), r);
        }
    }

    for tag in tag_map.into_values() {
        ref_map.entry(tag.target().clone()).or_default().push(tag);
    }

    ref_map.values_mut().for_each(|refs| refs.sort());

    cmd.wait().unwrap();

    (ref_map, head)
}

fn load_stashes_as_refs(path: &Path) -> RefMap {
    let format = ["%gd", "%H", "%s"].join("%x1f"); // use Unit Separator as a delimiter
    let mut cmd = Command::new("git")
        .arg("stash")
        .arg("list")
        .arg(format!("--format={format}"))
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");

    let reader = BufReader::new(stdout);

    let mut ref_map = RefMap::default();

    for line in reader.lines() {
        let line = line.unwrap();

        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() != 3 {
            panic!("unexpected number of parts: {} [{}]", parts.len(), line);
        }

        let name = parts[0];
        let hash = parts[1];
        let subject = parts[2];

        let r = Ref::Stash {
            name: name.into(),
            message: subject.into(),
            target: hash.into(),
        };

        ref_map.entry(hash.into()).or_default().push(r);
    }

    cmd.wait().unwrap();

    ref_map
}

fn merge_ref_maps(m1: &mut RefMap, m2: RefMap) {
    for (k, v) in m2 {
        m1.entry(k).or_default().extend(v);
    }
}

fn parse_branch_refs(hash: &str, refs: &str) -> Option<Ref> {
    if refs.starts_with("refs/heads/") {
        let name = refs.trim_start_matches("refs/heads/");
        Some(Ref::Branch {
            name: name.into(),
            target: hash.into(),
        })
    } else if refs.starts_with("refs/remotes/") {
        let name = refs.trim_start_matches("refs/remotes/");
        Some(Ref::RemoteBranch {
            name: name.into(),
            target: hash.into(),
        })
    } else {
        None
    }
}

fn parse_tag_refs(hash: &str, refs: &str) -> Option<Ref> {
    if refs.starts_with("refs/tags/") {
        let name = refs.trim_start_matches("refs/tags/");
        let name = name.trim_end_matches("^{}");
        Some(Ref::Tag {
            name: name.into(),
            target: hash.into(),
        })
    } else {
        None
    }
}

fn get_current_branch(path: &Path) -> Option<String> {
    let mut cmd = Command::new("git")
        .arg("branch")
        .arg("--show-current")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");

    let reader = BufReader::new(stdout);

    let branch = if let Some(line) = reader.lines().next() {
        line.ok()
    } else {
        None
    };

    cmd.wait().unwrap();

    branch
}

#[derive(Debug)]
pub enum FileChange {
    Add { path: String },
    Modify { path: String },
    Delete { path: String },
    Move { from: String, to: String },
}

impl FileChange {
    /// Renames open the destination's current working-tree version.
    pub fn path(&self) -> &str {
        match self {
            Self::Add { path } | Self::Modify { path } | Self::Delete { path } => path,
            Self::Move { to, .. } => to,
        }
    }
}

pub fn get_diff_summary(path: &Path, commit_hash: &CommitHash) -> Vec<FileChange> {
    let output = run_git(
        path,
        &[
            "diff",
            "--name-status",
            "-z",
            "--find-renames",
            "--no-relative",
            &format!("{}^", commit_hash.0),
            &commit_hash.0,
            "--",
        ],
    )
    .unwrap_or_default();
    parse_diff_summary(&output)
}

fn parse_diff_summary(output: &str) -> Vec<FileChange> {
    let mut fields = output.split_terminator('\0');
    let mut changes = Vec::new();
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let change = match status.as_bytes().first() {
            Some(b'A') => FileChange::Add { path: path.into() },
            Some(b'M' | b'T') => FileChange::Modify { path: path.into() },
            Some(b'D') => FileChange::Delete { path: path.into() },
            Some(b'R' | b'C') => {
                let Some(to) = fields.next() else { break };
                if status.starts_with('R') {
                    FileChange::Move {
                        from: path.into(),
                        to: to.into(),
                    }
                } else {
                    FileChange::Add { path: to.into() }
                }
            }
            _ => continue,
        };
        changes.push(change);
    }
    changes
}

pub fn get_initial_commit_additions(path: &Path, commit_hash: &CommitHash) -> Vec<FileChange> {
    run_git(
        path,
        &[
            "ls-tree",
            "--full-tree",
            "--name-only",
            "-r",
            "-z",
            &commit_hash.0,
        ],
    )
    .unwrap_or_default()
    .split_terminator('\0')
    .map(|path| FileChange::Add { path: path.into() })
    .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::test_support::git::GitRepository;

    fn git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(path)
            .env("GIT_AUTHOR_NAME", "Author Name")
            .env("GIT_AUTHOR_EMAIL", "author@example.com")
            .env("GIT_COMMITTER_NAME", "Committer Name")
            .env("GIT_COMMITTER_EMAIL", "committer@example.com")
            .env("GIT_CONFIG_NOSYSTEM", "true")
            .env("HOME", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit_file(path: &Path, name: &str, content: &str) {
        fs::write(path.join(name), content).unwrap();
        git(path, &["add", name]);
        git(path, &["commit", "-m", &format!("write {name}")]);
    }

    // master has file.txt = "master"; topic changes it to "topic".
    fn repository_with_topic_branch() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        GitRepository::new(path).init();
        commit_file(path, "file.txt", "master\n");
        git(path, &["switch", "-c", "topic"]);
        commit_file(path, "file.txt", "topic\n");
        git(path, &["switch", "master"]);
        dir
    }

    fn load(path: &Path) -> Repository {
        Repository::load(path, SortCommit::Chronological, None, false).unwrap()
    }

    fn current_branch(path: &Path) -> String {
        get_current_branch(path).unwrap()
    }

    #[test]
    fn test_parse_worktrees_reads_paths_and_branches() {
        let porcelain = "\
worktree /repo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /repo-wt/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/login

worktree /repo-wt/detached
HEAD 3333333333333333333333333333333333333333
detached
";
        let expected = vec![
            Worktree {
                path: "/repo".into(),
                branch: Some("main".into()),
                bare: false,
            },
            Worktree {
                path: "/repo-wt/feature".into(),
                branch: Some("feature/login".into()),
                bare: false,
            },
            Worktree {
                path: "/repo-wt/detached".into(),
                branch: None,
                bare: false,
            },
        ];
        assert_eq!(parse_worktrees(porcelain), expected);
    }

    #[test]
    fn test_changed_file_paths_preserve_spaces_controls_and_renames() {
        let changes = parse_diff_summary(
            "A\0space name\0M\0tab\tname\0D\0old\nname\0R100\0before\0after\0T\0symlink\0",
        );
        assert_eq!(
            changes.iter().map(FileChange::path).collect::<Vec<_>>(),
            ["space name", "tab\tname", "old\nname", "after", "symlink"]
        );
        assert!(
            matches!(&changes[3], FileChange::Move { from, to } if from == "before" && to == "after")
        );
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        let name = "file with\tspaces\nand newline.txt";
        fs::write(dir.path().join(name), "initial").unwrap();
        run_git(dir.path(), &["add", "."]).unwrap();
        git.commit("initial", "2026-01-01");
        let initial: CommitHash = run_git(dir.path(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .into();
        let changes = get_initial_commit_additions(dir.path(), &initial);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path(), name);
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        assert_eq!(
            get_initial_commit_additions(&subdir, &initial)[0].path(),
            name
        );
        // Repository-relative filenames must not depend on launch directory or Git config.
        run_git(dir.path(), &["config", "diff.relative", "true"]).unwrap();
        fs::write(dir.path().join(name), "modified").unwrap();
        run_git(dir.path(), &["add", "."]).unwrap();
        git.commit("modify", "2026-01-02");
        let head: CommitHash = run_git(dir.path(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .into();
        assert_eq!(get_diff_summary(dir.path(), &head)[0].path(), name);
        assert_eq!(get_diff_summary(&subdir, &head)[0].path(), name);
    }

    #[test]
    fn test_load_missing_worktree_returns_error_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("removed-worktree");
        assert!(Repository::load(&missing, SortCommit::Chronological, None, false).is_err());
        assert!(Repository::load(dir.path(), SortCommit::Chronological, None, false).is_err());
    }

    #[test]
    fn test_parse_worktrees_nul_paths_and_bare_repository() {
        let trees = parse_worktrees("worktree /bare\0bare\0\0worktree /agent\nwith\ttabs\0HEAD abc\0branch refs/heads/topic\0locked agent running\0\0worktree /detached\0detached\0prunable missing\0\0");
        assert_eq!(trees.len(), 3);
        assert!(trees[0].bare);
        assert_eq!(trees[1].path, PathBuf::from("/agent\nwith\ttabs"));
        assert_eq!(trees[1].branch.as_deref(), Some("topic"));
        assert_eq!(trees[2].branch, None);
    }

    #[test]
    fn test_worktree_root_from_subdirectory_and_fresh_listing() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        git.commit("initial", "2026-01-01");
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let repository = Repository::load(&subdir, SortCommit::Chronological, None, false).unwrap();
        assert_eq!(
            fs::canonicalize(repository.worktree_root().unwrap()).unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
        let other = tempfile::tempdir().unwrap();
        let path = other.path().join("agent with spaces");
        run_git(
            dir.path(),
            &["worktree", "add", "-b", "agent", path.to_str().unwrap()],
        )
        .unwrap();
        let trees = repository.worktrees().unwrap();
        assert_eq!(trees.len(), 2);
        assert!(trees.iter().any(|w| w.branch.as_deref() == Some("agent")));
        let agent = Repository::load(&path, SortCommit::Chronological, None, false).unwrap();
        assert!(matches!(agent.head(), Head::Branch { name } if name == "agent"));
        assert_eq!(
            repository.other_worktree_for_branch("master").unwrap(),
            None
        );
        assert_eq!(
            repository.other_worktree_for_branch("missing").unwrap(),
            None
        );
        assert_eq!(
            repository.other_worktree_for_branch("agent").unwrap(),
            Some(fs::canonicalize(&path).unwrap())
        );
        assert_eq!(agent.other_worktree_for_branch("agent").unwrap(), None);
        assert_eq!(
            agent.other_worktree_for_branch("master").unwrap(),
            Some(fs::canonicalize(dir.path()).unwrap())
        );
        assert_eq!(
            fs::canonicalize(agent.worktree_root().unwrap()).unwrap(),
            fs::canonicalize(&path).unwrap()
        );
    }

    #[test]
    fn test_other_worktree_branches_excludes_the_current_branch() {
        let worktrees = parse_worktrees(
            "worktree /repo\nbranch refs/heads/main\n\nworktree /wt\nbranch refs/heads/topic\n",
        );
        let head = Head::Branch {
            name: "topic".into(),
        };
        let actual = other_worktree_branches(worktrees, &head);
        let expected: FxHashMap<String, PathBuf> = [("main".to_string(), PathBuf::from("/repo"))]
            .into_iter()
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_git_error_message_fits_on_one_line() {
        let stderr = "error: Your local changes to the following files would be overwritten by checkout:\n\tfile.txt\nPlease commit your changes or stash them before you switch branches.\nAborting\n";
        assert_eq!(
            git_error_message(stderr, &["switch", "topic"]),
            "Your local changes to the following files would be overwritten by checkout: file.txt Please commit your changes or stash them before you switch branches. Aborting"
        );
        assert_eq!(
            git_error_message("fatal: invalid reference: nope\n", &["switch", "nope"]),
            "invalid reference: nope"
        );
        assert_eq!(
            git_error_message("", &["switch", "nope"]),
            "git switch nope failed"
        );
    }

    #[test]
    fn test_delete_branch_safe_delete_preserves_working_tree() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        git(path, &["branch", "merged/topic"]);
        fs::write(path.join("file.txt"), "agent changes\n").unwrap();
        let repository = load(path);
        repository
            .delete_branch(&local("merged/topic"), false)
            .unwrap();
        assert!(run_git(path, &["show-ref", "--verify", "refs/heads/merged/topic"]).is_err());
        assert_eq!(
            fs::read_to_string(path.join("file.txt")).unwrap(),
            "agent changes\n"
        );
        assert_eq!(current_branch(path), "master");
    }

    #[test]
    fn test_delete_unmerged_branch_requires_explicit_force() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let repository = load(path);
        assert!(matches!(
            repository.delete_branch(&local("topic"), false),
            Err(DeleteBranchError::NotMerged(_))
        ));
        assert!(run_git(path, &["show-ref", "--verify", "refs/heads/topic"]).is_ok());
        repository.delete_branch(&local("topic"), true).unwrap();
        assert!(run_git(path, &["show-ref", "--verify", "refs/heads/topic"]).is_err());
    }

    #[test]
    fn test_delete_branch_refuses_current_and_new_worktrees_even_with_force() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let repository = load(path);
        let worktree = tempfile::tempdir().unwrap();
        let worktree_path = worktree.path().join("agent");
        git(
            path,
            &["worktree", "add", worktree_path.to_str().unwrap(), "topic"],
        );
        for force in [false, true] {
            for name in ["master", "topic"] {
                let error = repository.delete_branch(&local(name), force).unwrap_err();
                assert!(
                    matches!(error, DeleteBranchError::Other(ref text) if text.contains("checked out in worktree")),
                    "{error:?}"
                );
                assert!(run_git(
                    path,
                    &["show-ref", "--verify", &format!("refs/heads/{name}")]
                )
                .is_ok());
            }
        }
    }

    #[test]
    fn test_delete_branch_refuses_stale_and_missing_selections() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let repository = load(path);
        git(path, &["branch", "-f", "topic", "master"]);
        let error = repository.delete_branch(&local("topic"), true).unwrap_err();
        assert!(
            matches!(error, DeleteBranchError::Other(ref text) if text.contains("changed since")),
            "{error:?}"
        );
        assert!(repository.delete_branch(&local("missing"), true).is_err());
        assert!(run_git(path, &["show-ref", "--verify", "refs/heads/topic"]).is_ok());
    }

    fn repository_with_remote_topic() -> (TempDir, TempDir) {
        let dir = repository_with_topic_branch();
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare"]);
        git(
            dir.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        );
        git(dir.path(), &["push", "origin", "topic"]);
        (dir, remote)
    }

    #[test]
    fn test_delete_remote_branch_leaves_local_branch_unchanged() {
        let (dir, remote) = repository_with_remote_topic();
        let repository = load(dir.path());
        let target = BranchTarget::Remote("origin/topic".into());
        assert!(repository.delete_branch(&target, true).is_err());
        assert!(run_git(remote.path(), &["show-ref", "--verify", "refs/heads/topic"]).is_ok());
        repository.delete_branch(&target, false).unwrap();
        assert!(run_git(remote.path(), &["show-ref", "--verify", "refs/heads/topic"]).is_err());
        assert!(run_git(dir.path(), &["show-ref", "--verify", "refs/heads/topic"]).is_ok());
    }

    #[test]
    fn test_delete_remote_branch_refuses_a_newer_server_tip() {
        let (dir, remote) = repository_with_remote_topic();
        let repository = load(dir.path());
        let new_tip = run_git(dir.path(), &["rev-parse", "master"]).unwrap();
        git(
            remote.path(),
            &["update-ref", "refs/heads/topic", new_tip.trim()],
        );
        let target = BranchTarget::Remote("origin/topic".into());
        assert!(repository.delete_branch(&target, false).is_err());
        assert_eq!(
            run_git(remote.path(), &["rev-parse", "refs/heads/topic"]).unwrap(),
            new_tip
        );
    }

    #[test]
    fn test_delete_remote_protects_active_local_upstream_with_a_different_name() {
        let (dir, remote) = repository_with_remote_topic();
        // A same-named tag must not make upstream branch names ambiguous.
        git(dir.path(), &["tag", "master"]);
        git(
            dir.path(),
            &["branch", "--set-upstream-to=origin/topic", "master"],
        );
        let repository = load(dir.path());
        let target = BranchTarget::Remote("origin/topic".into());
        let error = repository.delete_branch(&target, false).unwrap_err();
        assert!(
            matches!(error, DeleteBranchError::Other(ref text) if text.contains("'master' tracks 'origin/topic' and is checked out in worktree")),
            "{error:?}"
        );
        assert!(run_git(remote.path(), &["show-ref", "--verify", "refs/heads/topic"]).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_delete_remote_branch_cannot_prompt_on_the_terminal() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let topic = run_git(path, &["rev-parse", "topic"]).unwrap();
        git(
            path,
            &["update-ref", "refs/remotes/origin/topic", topic.trim()],
        );
        git(path, &["remote", "add", "origin", "prompt-host:repo.git"]);
        // Stands in for SSH: record whether a passphrase prompt could reach a terminal.
        // Isolate `:` in a subshell: dash exits on a special builtin's redirection
        // failure, even inside `if`. The parent must still record no-tty and the
        // prompt setting. Redirect stderr outside the subshell to silence that failure.
        let probe = path.join("tty-probe");
        let ssh = path.join("fake-ssh");
        fs::write(
            &ssh,
            format!(
                "#!/bin/sh\nif ( : </dev/tty ) 2>/dev/null; then echo tty; else echo no-tty; fi > '{}'\necho \"$GIT_TERMINAL_PROMPT\" >> '{}'\nexit 1\n",
                probe.display(),
                probe.display()
            ),
        )
        .unwrap();
        git(path, &["config", "core.sshCommand", "sh fake-ssh"]);
        let repository = load(path);
        let target = BranchTarget::Remote("origin/topic".into());
        assert!(repository.delete_branch(&target, false).is_err());
        assert_eq!(fs::read_to_string(probe).unwrap(), "no-tty\n0\n");
    }

    #[test]
    fn test_delete_remote_rejects_head_alias() {
        let (dir, _) = repository_with_remote_topic();
        git(
            dir.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/topic",
            ],
        );
        let repository = load(dir.path());
        assert!(repository
            .delete_branch(&BranchTarget::Remote("origin/HEAD".into()), false)
            .is_err());
        let tip = repository
            .all_refs()
            .into_iter()
            .find(|r| r.name() == "topic")
            .unwrap()
            .target()
            .clone();
        let choices = repository.branches_for_deletion(&tip);
        assert!(choices.contains(&local("topic")));
        assert!(choices.contains(&BranchTarget::Remote("origin/topic".into())));
        assert!(!choices.contains(&BranchTarget::Remote("origin/HEAD".into())));
    }

    #[test]
    fn test_switch_branch_switches_to_a_local_branch() {
        let dir = repository_with_topic_branch();
        let repository = load(dir.path());

        repository
            .switch_branch(&BranchTarget::Local("topic".into()))
            .unwrap();

        assert_eq!(current_branch(dir.path()), "topic");
        assert!(matches!(load(dir.path()).head(), Head::Branch { name } if name == "topic"));
    }

    #[test]
    fn test_switch_branch_reports_unknown_branch() {
        let dir = repository_with_topic_branch();
        let repository = load(dir.path());

        let err = repository
            .switch_branch(&BranchTarget::Local("missing".into()))
            .unwrap_err();

        assert!(err.contains("missing"), "{err}");
        assert_eq!(current_branch(dir.path()), "master");
    }

    #[test]
    fn test_switch_branch_keeps_conflicting_local_changes() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        fs::write(path.join("file.txt"), "edited\n").unwrap();
        let repository = load(path);
        assert!(repository.has_uncommitted_changes().unwrap());

        let err = repository
            .switch_branch(&BranchTarget::Local("topic".into()))
            .unwrap_err();

        assert!(err.contains("would be overwritten"), "{err}");
        assert_eq!(current_branch(path), "master");
        assert_eq!(
            fs::read_to_string(path.join("file.txt")).unwrap(),
            "edited\n"
        );
    }

    #[test]
    fn test_graph_lanes_are_saved_and_forgotten() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        assert_eq!(saved_graph_lanes(path), None);
        // forgetting when nothing is saved is not an error
        save_graph_lanes(path, None).unwrap();

        save_graph_lanes(path, Some(3)).unwrap();
        assert_eq!(saved_graph_lanes(path), Some(3));
        save_graph_lanes(path, Some(5)).unwrap();
        assert_eq!(saved_graph_lanes(path), Some(5));

        save_graph_lanes(path, None).unwrap();
        assert_eq!(saved_graph_lanes(path), None);
    }

    #[test]
    fn test_saved_graph_lanes_ignores_unusable_values() {
        let dir = repository_with_topic_branch();
        let path = dir.path();

        git(path, &["config", "--local", "serie.graphLanes", "0"]);
        assert_eq!(saved_graph_lanes(path), None);
        git(path, &["config", "--local", "serie.graphLanes", "wide"]);
        assert_eq!(saved_graph_lanes(path), None);
    }

    #[test]
    fn test_has_uncommitted_changes_ignores_untracked_files() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let repository = load(path);
        assert!(!repository.has_uncommitted_changes().unwrap());

        fs::write(path.join("untracked.txt"), "new\n").unwrap();
        assert!(!repository.has_uncommitted_changes().unwrap());

        fs::write(path.join("staged.txt"), "new\n").unwrap();
        git(path, &["add", "staged.txt"]);
        assert!(repository.has_uncommitted_changes().unwrap());
    }

    #[test]
    fn test_switch_branch_tracks_a_remote_branch() {
        let origin = repository_with_topic_branch();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clone");
        git(
            dir.path(),
            &["clone", "-q", origin.path().to_str().unwrap(), "clone"],
        );
        let repository = load(&path);

        repository
            .switch_branch(&BranchTarget::Remote("origin/topic".into()))
            .unwrap();

        assert_eq!(current_branch(&path), "topic");
        assert_eq!(
            fs::read_to_string(path.join("file.txt")).unwrap(),
            "topic\n"
        );

        // A local branch of that name now exists, so git refuses to create it again.
        git(&path, &["switch", "master"]);
        let err = load(&path)
            .switch_branch(&BranchTarget::Remote("origin/topic".into()))
            .unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(current_branch(&path), "master");
    }

    #[test]
    fn test_load_finds_branches_checked_out_in_other_worktrees() {
        let dir = repository_with_topic_branch();
        let path = dir.path();
        let worktree_dir = tempfile::tempdir().unwrap();
        let worktree_path = worktree_dir.path().join("topic-wt");
        git(
            path,
            &[
                "worktree",
                "add",
                "-q",
                worktree_path.to_str().unwrap(),
                "topic",
            ],
        );

        let from_main = load(path);
        let branches: Vec<&String> = from_main.other_worktree_branches().keys().collect();
        assert_eq!(branches, vec!["topic"]);
        let listed_path = fs::canonicalize(&from_main.other_worktree_branches()["topic"]).unwrap();
        assert_eq!(listed_path, fs::canonicalize(&worktree_path).unwrap());

        let from_worktree = load(&worktree_path);
        let branches: Vec<&String> = from_worktree.other_worktree_branches().keys().collect();
        assert_eq!(branches, vec!["master"]);

        // Git itself also refuses, which is the backstop if the list is stale.
        let err = from_main
            .switch_branch(&BranchTarget::Local("topic".into()))
            .unwrap_err();
        assert!(err.contains("topic"), "{err}");
        assert_eq!(current_branch(path), "master");
    }

    fn branch(name: &str) -> Ref {
        Ref::Branch {
            name: name.into(),
            target: CommitHash::from("a"),
        }
    }

    fn remote_branch(name: &str) -> Ref {
        Ref::RemoteBranch {
            name: name.into(),
            target: CommitHash::from("a"),
        }
    }

    fn local(name: &str) -> BranchTarget {
        BranchTarget::Local(name.into())
    }

    #[test]
    fn test_branch_names_for_copy_prefers_locals_and_includes_current() {
        let refs = [
            branch("topic"),
            remote_branch("origin/main"),
            branch("main"),
        ];
        let refs: Vec<&Ref> = refs.iter().collect();
        assert_eq!(branch_names_for_copy(&refs), vec!["main", "topic"]);
    }

    #[test]
    fn test_branch_names_for_copy_falls_back_to_remote_names() {
        let refs = [
            remote_branch("origin/topic"),
            remote_branch("origin/HEAD"),
            remote_branch("upstream/main"),
        ];
        let refs: Vec<&Ref> = refs.iter().collect();
        assert_eq!(
            branch_names_for_copy(&refs),
            vec!["origin/topic", "upstream/main"]
        );
    }

    #[test]
    fn test_branch_names_for_copy_ignores_tags_stashes_and_remote_head() {
        let target = CommitHash::from("unused");
        let refs = [
            Ref::Tag {
                name: "v1".into(),
                target: target.clone(),
            },
            Ref::Stash {
                name: "stash@{0}".into(),
                message: "saved".into(),
                target,
            },
            remote_branch("origin/HEAD"),
        ];
        let refs: Vec<&Ref> = refs.iter().collect();
        assert!(branch_names_for_copy(&refs).is_empty());
        assert!(branch_names_for_copy(&[]).is_empty());
    }

    #[test]
    fn test_branches_at_commit_single_local_branch() {
        let refs = [branch("topic"), remote_branch("origin/topic")];
        let refs: Vec<&Ref> = refs.iter().collect();

        let actual = branches_at_commit(&refs, &["main", "topic"], Some("main"));

        // the remote branch is covered by the local branch at the same commit
        let expected = BranchesAtCommit {
            targets: vec![local("topic")],
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_branches_at_commit_skips_current_branch() {
        let refs = [branch("main"), remote_branch("origin/main")];
        let refs: Vec<&Ref> = refs.iter().collect();

        let actual = branches_at_commit(&refs, &["main"], Some("main"));

        let expected = BranchesAtCommit {
            has_current: true,
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_branches_at_commit_orders_locals_before_remotes() {
        let refs = [
            remote_branch("origin/feature/new"),
            branch("b"),
            remote_branch("origin/HEAD"),
            branch("a"),
            Ref::Tag {
                name: "v1".into(),
                target: CommitHash::from("a"),
            },
        ];
        let refs: Vec<&Ref> = refs.iter().collect();

        let actual = branches_at_commit(&refs, &["a", "b", "main"], Some("main"));

        let expected = BranchesAtCommit {
            targets: vec![
                local("a"),
                local("b"),
                BranchTarget::Remote("origin/feature/new".into()),
            ],
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_branches_at_commit_remote_with_local_branch_elsewhere() {
        let refs = [remote_branch("origin/topic")];
        let refs: Vec<&Ref> = refs.iter().collect();

        let actual = branches_at_commit(&refs, &["main", "topic"], Some("main"));

        let expected = BranchesAtCommit {
            shadowed_remotes: vec![("origin/topic".into(), "topic".into())],
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_branches_at_commit_without_branch() {
        let actual = branches_at_commit(&[], &["main"], None);

        assert_eq!(actual, BranchesAtCommit::default());
    }
}
