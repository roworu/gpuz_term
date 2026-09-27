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

#[cfg(test)]
mod tests {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    use super::*;

    /// kills and reaps the child when the test ends, even on panic
    struct Kill(Child);

    impl Drop for Kill {
        fn drop(&mut self) {
            self.0.kill().ok();
            self.0.wait().ok();
        }
    }

    fn wait_for(pid: u32, want: &str) -> Option<ForegroundProcess> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let p = foreground_process(pid);
            if p.as_ref().is_some_and(|p| p.name == want) || Instant::now() > deadline {
                return p;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // with a controlling terminal the tty's foreground process is reported instead of the pid,
    // so name checks only work without one, like in the container
    fn has_no_ctty() -> bool {
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        stat.rsplit_once(')').unwrap().1.split_whitespace().nth(5).unwrap() == "-1"
    }

    #[test]
    fn missing_pid_is_none() {
        assert!(foreground_process(u32::MAX - 1).is_none());
        assert!(foreground_process(0).is_none());
    }

    #[test]
    fn comm_with_parens_and_spaces_parses() {
        if !has_no_ctty() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("gpuz_term_proc_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("we ird) (x 1 2");
        std::fs::copy("/bin/sleep", &exe).unwrap();
        // other tests fork while the copy's write fd is open, so exec may briefly fail with ETXTBSY
        let mut tries = 0;
        let child = loop {
            match Command::new(&exe).arg("10").current_dir(&dir).spawn() {
                Ok(child) => break Kill(child),
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 100 => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => panic!("{e}"),
            }
        };
        let p = wait_for(child.0.id(), "we ird) (x 1 2").expect("parsed");
        assert_eq!(p.name, "we ird) (x 1 2");
        assert_eq!(p.cwd.as_deref(), Some(dir.as_path()));
        drop(child);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn login_dash_and_path_are_stripped() {
        if !has_no_ctty() {
            return;
        }
        let child = Kill(
            Command::new("bash")
                .args(["-c", "exec -a -/usr/bin/mysleep sleep 10"])
                .spawn()
                .unwrap(),
        );
        let p = wait_for(child.0.id(), "mysleep").expect("some");
        assert_eq!(p.name, "mysleep");
    }
}
