//! building tab titles from the blocks set in settings

use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{settings::TabTitleBlock, terminal::foreground_process};

// a hung command would freeze every tab title, so it is abandoned after this
const EXEC_TIMEOUT: Duration = Duration::from_secs(2);

/// what a title is built from, collected on the main thread
pub(super) struct TitleInputs {
    pub number: usize,
    pub shell_pid: u32,
    pub title: String,
}

/// join blocks into a title, may block on /proc reads and exec commands
pub(super) fn build_title(blocks: &[TabTitleBlock], inputs: &TitleInputs) -> String {
    let process = foreground_process(inputs.shell_pid);
    let cwd = process.as_ref().and_then(|process| process.cwd.as_deref());
    let mut title = String::new();
    for block in blocks {
        match block {
            TabTitleBlock::Number => title.push_str(&inputs.number.to_string()),
            TabTitleBlock::Prompt => title.push_str(&prompt()),
            TabTitleBlock::Folder => title.push_str(&cwd.map(folder_name).unwrap_or_default()),
            TabTitleBlock::Command => {
                title.push_str(process.as_ref().map_or("", |process| &process.name))
            }
            TabTitleBlock::Title => title.push_str(&inputs.title),
            TabTitleBlock::Text(text) => title.push_str(text),
            TabTitleBlock::Exec(command) => title.push_str(&exec(command, cwd).unwrap_or_default()),
        }
    }
    title
}

fn prompt() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .unwrap_or_default();
    format!("{user}@{}", host.trim())
}

fn folder_name(cwd: &Path) -> String {
    if std::env::var_os("HOME").is_some_and(|home| cwd == Path::new(&home)) {
        return "~".to_string();
    }
    cwd.file_name()
        .map_or_else(|| cwd.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn exec(command: &str, cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // read on another thread, so a full pipe can't stall the child until the timeout
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).ok();
        output_tx.send(output).ok();
    });
    let deadline = Instant::now() + EXEC_TIMEOUT;
    while let Ok(None) = child.try_wait() {
        if Instant::now() > deadline {
            child.kill().ok();
            child.wait().ok();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // background jobs of sh may still hold the pipe, so the reader is left to finish alone
    let output = output_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok()?;
    Some(String::from_utf8_lossy(&output).lines().next()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(blocks: &[TabTitleBlock]) -> String {
        let inputs = TitleInputs {
            number: 3,
            shell_pid: std::process::id(),
            title: "vim".into(),
        };
        build_title(blocks, &inputs)
    }

    #[test]
    fn joins_blocks_in_order() {
        let title = build(&[
            TabTitleBlock::Number,
            TabTitleBlock::Text(": ".into()),
            TabTitleBlock::Title,
            TabTitleBlock::Text(" - ".into()),
            TabTitleBlock::Exec("echo first; echo second".into()),
        ]);
        assert_eq!(title, "3: vim - first");
    }

    #[test]
    fn folder_and_exec_use_process_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let folder = cwd.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(build(&[TabTitleBlock::Folder]), folder);
        assert_eq!(build(&[TabTitleBlock::Exec("pwd".into())]), cwd.display().to_string());
    }

    #[test]
    fn folder_name_shortens_home_and_root() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(folder_name(Path::new(&home)), "~");
        assert_eq!(folder_name(Path::new("/")), "/");
        assert_eq!(folder_name(Path::new("/usr/lib")), "lib");
    }

    #[test]
    fn prompt_is_user_at_host() {
        let prompt = build(&[TabTitleBlock::Prompt]);
        let (user, host) = prompt.split_once('@').unwrap();
        assert_eq!(user, std::env::var("USER").unwrap_or_default());
        assert!(!host.is_empty());
    }

    #[test]
    fn failing_and_hung_commands_are_empty() {
        assert_eq!(build(&[TabTitleBlock::Exec("exit 1".into())]), "");
        let start = Instant::now();
        assert_eq!(build(&[TabTitleBlock::Exec("sleep 10".into())]), "");
        assert_eq!(build(&[TabTitleBlock::Exec("echo hi; sleep 10 &".into())]), "");
        assert!(start.elapsed() < Duration::from_secs(8));
    }
}
