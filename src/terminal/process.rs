//! finding what runs in the foreground of a shell, read from /proc so linux only

use std::path::{Path, PathBuf};

/// program in the foreground of a terminal
#[derive(Debug)]
pub struct ForegroundProcess {
    pub name: String,
    pub cwd: Option<PathBuf>,
}

/// foreground process of the terminal `shell_pid` runs in, none when /proc can't tell
pub fn foreground_process(shell_pid: u32) -> Option<ForegroundProcess> {
    let stat = std::fs::read_to_string(format!("/proc/{shell_pid}/stat")).ok()?;
    // comm may contain spaces and parens, so count fields after the last ')'
    let tpgid: i32 = stat.rsplit_once(')')?.1.split_whitespace().nth(5)?.parse().ok()?;
    // -1 when there is no controlling terminal, then the shell itself is shown
    let pid = if tpgid > 0 { tpgid as u32 } else { shell_pid };
    let name = process_name(pid)?;
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    Some(ForegroundProcess { name, cwd })
}

fn process_name(pid: u32) -> Option<String> {
    // comm is cut to 15 bytes, so prefer argv[0] and use comm only as a fallback
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let arg0 = String::from_utf8_lossy(cmdline.split(|b| *b == 0).next()?).into_owned();
    // login shells start with a dash in argv[0], like "-bash"
    let name = Path::new(arg0.trim_start_matches('-'))
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    name.or_else(|| {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    })
}
