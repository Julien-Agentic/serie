use std::{
    cell::RefCell,
    path::{Component, Path, PathBuf},
    process::Command,
};

use arboard::Clipboard;

use crate::config::ClipboardConfig;

/// Resolve an existing working-tree file, never materialize historical content.
pub fn working_tree_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Invalid repository-relative file path".into());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot open worktree: {error}"))?;
    let path = root.join(relative);
    let resolved = path.canonicalize().map_err(|error| {
        format!(
            "Working-tree file {} is unavailable: {error}. Historical content is not opened",
            path.display()
        )
    })?;
    if !resolved.starts_with(&root) {
        return Err("File resolves outside the selected worktree; not opening".into());
    }
    if !resolved.is_file() {
        return Err("Selected path is not a regular working-tree file (directories/submodules are not opened)".into());
    }
    // Keep the repository pathname for editors rather than the symlink target.
    Ok(path)
}

fn preferred_editor(visual: Option<String>, editor: Option<String>) -> String {
    visual
        .filter(|value| !value.trim().is_empty())
        .or_else(|| editor.filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "vi".into())
}

fn editor_command(root: &Path, file: &Path, editor: &str) -> Command {
    // The editor is trusted user configuration and may contain flags/quoting.
    // The filename is passed separately, never interpolated into shell source.
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!("exec {editor} \"$@\""))
        .arg("serie-editor")
        .arg(file)
        .current_dir(root);
    command
}

pub fn exec_editor(root: &Path, file: &Path) -> Result<(), String> {
    let editor = preferred_editor(std::env::var("VISUAL").ok(), std::env::var("EDITOR").ok());
    let status = editor_command(root, file, &editor)
        .status()
        .map_err(|error| format!("Failed to launch editor: {error}"))?;
    if !status.success() {
        return Err(format!("Editor exited with {status}"));
    }
    Ok(())
}

const USER_COMMAND_MARKER_PREFIX: &str = "{{";
const USER_COMMAND_TARGET_HASH_MARKER: &str = "{{target_hash}}";
const USER_COMMAND_MARKED_HASH_MARKER: &str = "{{marked_hash}}";
const USER_COMMAND_FIRST_PARENT_HASH_MARKER: &str = "{{first_parent_hash}}";
const USER_COMMAND_PARENT_HASHES_MARKER: &str = "{{parent_hashes}}";
const USER_COMMAND_REFS_MARKER: &str = "{{refs}}";
const USER_COMMAND_BRANCHES_MARKER: &str = "{{branches}}";
const USER_COMMAND_REMOTE_BRANCHES_MARKER: &str = "{{remote_branches}}";
const USER_COMMAND_TAGS_MARKER: &str = "{{tags}}";
const USER_COMMAND_STASH_MARKER: &str = "{{stash}}";
const USER_COMMAND_AREA_WIDTH_MARKER: &str = "{{area_width}}";
const USER_COMMAND_AREA_HEIGHT_MARKER: &str = "{{area_height}}";

thread_local! {
    static CLIPBOARD: RefCell<Option<Clipboard>> = const { RefCell::new(None) };
}

pub fn copy_to_clipboard(value: String, config: &ClipboardConfig) -> Result<(), String> {
    match config {
        ClipboardConfig::Auto => copy_to_clipboard_auto(value),
        ClipboardConfig::Custom { commands } => copy_to_clipboard_custom(value, commands),
    }
}

fn copy_to_clipboard_custom(value: String, commands: &[String]) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    if commands.is_empty() {
        return Err("No clipboard command specified".to_string());
    }

    let mut child = Command::new(&commands[0])
        .args(&commands[1..])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run {}: {e}", commands[0]))?;

    child
        .stdin
        .take()
        .expect("stdin should be available")
        .write_all(value.as_bytes())
        .map_err(|e| format!("Failed to write to {}: {e}", commands[0]))?;

    child
        .wait()
        .map_err(|e| format!("{} failed: {e}", commands[0]))?;

    Ok(())
}

fn copy_to_clipboard_auto(value: String) -> Result<(), String> {
    CLIPBOARD.with_borrow_mut(|clipboard| {
        if clipboard.is_none() {
            *clipboard = Clipboard::new()
                .map(Some)
                .map_err(|e| format!("Failed to create clipboard: {e:?}"))?;
        }

        clipboard
            .as_mut()
            .expect("The clipboard should have been initialized above")
            .set_text(value)
            .map_err(|e| format!("Failed to copy to clipboard: {e:?}"))
    })
}

pub struct ExternalCommandParameters<'a> {
    pub command: &'a [String],
    pub target_hash: &'a str,
    pub marked_hash: Option<&'a str>,
    pub parent_hashes: Vec<&'a str>,
    pub all_refs: Vec<&'a str>,
    pub branches: Vec<&'a str>,
    pub remote_branches: Vec<&'a str>,
    pub tags: Vec<&'a str>,
    pub stash: Option<&'a str>,
    pub area_width: u16,
    pub area_height: u16,
}

pub fn exec_user_command(params: ExternalCommandParameters) -> Result<String, String> {
    let command = build_user_command(&params);

    let output = Command::new(&command[0])
        .args(&command[1..])
        .output()
        .map_err(|e| format!("Failed to execute command: {e:?}"))?;

    if !output.status.success() {
        let msg = format!(
            "Command exited with non-zero status: {}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(msg);
    }

    Ok(String::from_utf8_lossy(&output.stdout).into())
}

pub fn exec_user_command_suspend(params: ExternalCommandParameters) -> Result<(), String> {
    let command = build_user_command(&params);

    let output = Command::new(&command[0])
        .args(&command[1..])
        .status()
        .map_err(|e| format!("Failed to execute command: {e:?}"))?;

    if !output.success() {
        let msg = format!("Command exited with non-zero status: {output}");
        return Err(msg);
    }

    Ok(())
}

fn build_user_command(params: &ExternalCommandParameters) -> Vec<String> {
    fn to_vec(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }
    let mut command = Vec::new();
    for arg in params.command {
        if !arg.contains(USER_COMMAND_MARKER_PREFIX) {
            command.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            // If the marker is used as a standalone argument, expand it into multiple arguments.
            // This allows the command to receive each item as a separate argument and correctly handle items that contain spaces.
            USER_COMMAND_BRANCHES_MARKER => command.extend(to_vec(&params.branches)),
            USER_COMMAND_REMOTE_BRANCHES_MARKER => command.extend(to_vec(&params.remote_branches)),
            USER_COMMAND_TAGS_MARKER => command.extend(to_vec(&params.tags)),
            USER_COMMAND_REFS_MARKER => command.extend(to_vec(&params.all_refs)),
            USER_COMMAND_PARENT_HASHES_MARKER => command.extend(to_vec(&params.parent_hashes)),
            // Otherwise, replace the marker within the single argument string.
            _ => command.push(replace_command_arg(arg, params)),
        }
    }
    command
}

fn replace_command_arg(s: &str, params: &ExternalCommandParameters) -> String {
    let sep = " ";
    let target_hash = params.target_hash;
    let marked_hash = params.marked_hash.unwrap_or_default();
    let first_parent_hash = &params.parent_hashes.first().cloned().unwrap_or_default();
    let parent_hashes = &params.parent_hashes.join(sep);
    let all_refs = &params.all_refs.join(sep);
    let branches = &params.branches.join(sep);
    let remote_branches = &params.remote_branches.join(sep);
    let tags = &params.tags.join(sep);
    let stash = params.stash.unwrap_or_default();
    let area_width = &params.area_width.to_string();
    let area_height = &params.area_height.to_string();

    s.replace(USER_COMMAND_TARGET_HASH_MARKER, target_hash)
        .replace(USER_COMMAND_MARKED_HASH_MARKER, marked_hash)
        .replace(USER_COMMAND_FIRST_PARENT_HASH_MARKER, first_parent_hash)
        .replace(USER_COMMAND_PARENT_HASHES_MARKER, parent_hashes)
        .replace(USER_COMMAND_REFS_MARKER, all_refs)
        .replace(USER_COMMAND_BRANCHES_MARKER, branches)
        .replace(USER_COMMAND_REMOTE_BRANCHES_MARKER, remote_branches)
        .replace(USER_COMMAND_TAGS_MARKER, tags)
        .replace(USER_COMMAND_STASH_MARKER, stash)
        .replace(USER_COMMAND_AREA_WIDTH_MARKER, area_width)
        .replace(USER_COMMAND_AREA_HEIGHT_MARKER, area_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_precedence_and_fallback() {
        assert_eq!(
            preferred_editor(Some("nvim -f".into()), Some("vim".into())),
            "nvim -f"
        );
        assert_eq!(
            preferred_editor(Some(" ".into()), Some("vim".into())),
            "vim"
        );
        assert_eq!(preferred_editor(None, Some("".into())), "vi");
    }

    #[test]
    fn test_editor_gets_literal_filename_and_worktree_directory() {
        let root = tempfile::tempdir().unwrap();
        let name = "file ' ; $(touch injected)\t\n.txt";
        std::fs::write(root.path().join(name), "current version").unwrap();
        let path = working_tree_file(root.path(), Path::new(name)).unwrap();
        let output = editor_command(
            root.path(),
            &path,
            "sh -c 'printf \"%s\\n\" \"$PWD\" \"$@\"' editor-test",
        )
        .output()
        .unwrap();
        assert!(output.status.success());
        let expected = format!(
            "{}\n{}\n",
            root.path().canonicalize().unwrap().display(),
            path.display()
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert!(!root.path().join("injected").exists());
    }

    #[test]
    fn test_missing_directories_and_escaping_paths_are_not_opened() {
        let root = tempfile::tempdir().unwrap();
        for relative in ["deleted.txt", "../outside", "/absolute", "", "."] {
            assert!(working_tree_file(root.path(), Path::new(relative)).is_err());
        }
        std::fs::create_dir(root.path().join("submodule")).unwrap();
        assert!(working_tree_file(root.path(), Path::new("submodule")).is_err());
        assert!(!root.path().join("deleted.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_outside_worktree_is_not_opened() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        assert!(working_tree_file(root.path(), Path::new("link")).is_err());
    }
}
