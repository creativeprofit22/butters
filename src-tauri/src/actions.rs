//! Closing things. Every request is re-checked against the live system at
//! the moment it runs:
//! - each pid must still belong to the *same* process (pid + start time),
//!   so a reused pid can never hit an unrelated program;
//! - every process must belong to the signed-in user;
//! - Butters, whatever launched it, and the app in front are refused;
//! - pids 0 and 1 (and 0–4 on Windows) are refused outright (signals to
//!   them mean "everyone" or the kernel).
//!
//! Polite first: apps get the same Quit request as their menu, command-line
//! tools get SIGTERM. Forcing is a separate, explicit request. A forced close
//! also stops live descendants started since the scan, found over verified
//! parent→child edges and each re-checked like a member; one protected or
//! foreign descendant refuses the whole close. Each kill re-checks the start
//! time right before it, so a reused pid is never signalled.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::platform;

/// What the panel asked to close, as captured by the scan that suggested it.
#[derive(Debug, Clone)]
pub struct ProcTarget {
    pub title: String,
    /// (pid, start_time) for every process in the group, root first.
    pub members: Vec<(u32, u64)>,
    pub app_pid: Option<u32>,
    pub memory: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// Everything exited. `freed` is the measured memory of what exited;
    /// `disk` is disk space given back (Docker clean-ups).
    Closed {
        message: String,
        freed: u64,
        disk: u64,
    },
    /// Asked politely, but it's still running (maybe asking to save).
    StillRunning {
        message: String,
    },
    /// Can't be asked politely; only a forced close would work.
    NeedsForce {
        message: String,
    },
    /// A safety rule blocked it.
    Refused {
        message: String,
    },
    Failed {
        message: String,
    },
}

/// pids that must never be touched, computed fresh at action time.
pub struct Guard {
    pub forbidden: HashSet<u32>,
}

impl Guard {
    pub fn current(frontmost: Option<u32>) -> Guard {
        let mut forbidden: HashSet<u32> = [0, 1].into_iter().collect();
        let me = std::process::id();
        forbidden.insert(me);
        // Butters' ancestors (the terminal it was started from, in dev).
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let mut cur = Pid::from_u32(me);
        for _ in 0..64 {
            match sys.process(cur).and_then(|p| p.parent()) {
                Some(parent) if forbidden.insert(parent.as_u32()) => cur = parent,
                _ => break,
            }
        }
        if let Some(f) = frontmost {
            forbidden.insert(f);
        }
        Guard { forbidden }
    }
}

#[derive(Clone)]
struct Live {
    pid: u32,
    /// Start time as the scan (or the close-time walk) saw it.
    start: u64,
    memory: u64,
}

/// pids that address the kernel, init or "everyone"; never signalled.
fn is_system_pid(pid: u32) -> bool {
    if cfg!(windows) {
        pid <= 4 // 0 is the idle process, 4 is System
    } else {
        pid <= 1
    }
}

/// Checks every member against the live system. Returns the processes that
/// are still the same ones the scan saw, or why the request is refused.
fn validate(target: &ProcTarget, guard: &Guard) -> Result<Vec<Live>, String> {
    let pids: Vec<Pid> = target
        .members
        .iter()
        .map(|(p, _)| Pid::from_u32(*p))
        .collect();
    let mut sys = System::new();
    let me = Pid::from_u32(std::process::id());
    let mut with_me = pids.clone();
    with_me.push(me);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&with_me),
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_user(UpdateKind::Always)
            .without_tasks(),
    );
    let my_uid = sys.process(me).and_then(|p| p.user_id().cloned());
    let Some(my_uid) = my_uid else {
        return Err("Can't verify owner, skipped".into());
    };
    let mut live = Vec::new();
    for (pid, start) in &target.members {
        if guard.forbidden.contains(pid) || is_system_pid(*pid) {
            return Err("Protected, skipped".to_string());
        }
        let Some(p) = sys.process(Pid::from_u32(*pid)) else {
            continue;
        }; // already gone
        if p.start_time() != *start {
            continue; // pid now belongs to a different program: leave it alone
        }
        if p.user_id() != Some(&my_uid) {
            return Err("Not yours, skipped".to_string());
        }
        live.push(Live {
            pid: *pid,
            start: *start,
            memory: platform::footprint(*pid).unwrap_or_else(|| p.memory()),
        });
    }
    Ok(live)
}

/// One row of the live process table: (pid, parent, start time).
type Row = (u32, Option<u32>, u64);

/// How far below a member the close-time walk looks.
const MAX_DEPTH: usize = 64;

/// Every process below `roots` reachable over verified parent→child edges,
/// root-first (level by level, by pid within a level). An edge counts only
/// when both start times are known, the child started no earlier than its
/// parent, and the parent is still the process we walked from (same start),
/// so a reused pid never pulls in an unrelated tree. Roots are not repeated.
fn descendants(table: &[Row], roots: &[(u32, u64)]) -> Vec<(u32, u64)> {
    let mut start_of: HashMap<u32, u64> = HashMap::new();
    let mut children: HashMap<u32, Vec<(u32, u64)>> = HashMap::new();
    for &(pid, parent, start) in table {
        start_of.insert(pid, start);
        if let Some(parent) = parent.filter(|p| *p != pid) {
            children.entry(parent).or_default().push((pid, start));
        }
    }
    let mut seen: HashSet<u32> = roots.iter().map(|(p, _)| *p).collect();
    let mut out = Vec::new();
    let mut level: Vec<(u32, u64)> = roots.to_vec();
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for &(pid, start) in &level {
            if start == 0 || start_of.get(&pid) != Some(&start) {
                continue;
            }
            for &(child, child_start) in children.get(&pid).map_or(&[][..], Vec::as_slice) {
                if child_start != 0 && child_start >= start && seen.insert(child) {
                    next.push((child, child_start));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_unstable();
        out.extend_from_slice(&next);
        level = next;
    }
    out
}

/// Live descendants of `live` (validated members), found in a fresh process
/// table and each re-checked like a member. Any protected or foreign one
/// refuses the whole close, before anything is signalled.
fn live_descendants(live: &[Live], guard: &Guard) -> Result<Vec<Live>, String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_user(UpdateKind::Always)
            .without_tasks(),
    );
    let me = Pid::from_u32(std::process::id());
    let Some(my_uid) = sys.process(me).and_then(|p| p.user_id().cloned()) else {
        return Err("Can't verify owner, skipped".into());
    };
    let table: Vec<Row> = sys
        .processes()
        .values()
        .map(|p| {
            (
                p.pid().as_u32(),
                p.parent().map(Pid::as_u32),
                p.start_time(),
            )
        })
        .collect();
    let roots: Vec<(u32, u64)> = live.iter().map(|l| (l.pid, l.start)).collect();
    let mut found = Vec::new();
    for (pid, start) in descendants(&table, &roots) {
        if guard.forbidden.contains(&pid) || is_system_pid(pid) {
            return Err("Protected, skipped".to_string());
        }
        let Some(p) = sys.process(Pid::from_u32(pid)) else {
            continue;
        };
        if is_zombie(p) {
            continue; // already exited
        }
        if p.user_id() != Some(&my_uid) {
            return Err("Not yours, skipped".to_string());
        }
        found.push(Live {
            pid,
            start,
            memory: platform::footprint(pid).unwrap_or_else(|| p.memory()),
        });
    }
    Ok(found)
}

#[cfg(unix)]
pub fn send_signal(pid: u32, sig: libc::c_int) -> Result<(), String> {
    // pid 0 / 1 / negative values address whole process groups or every
    // process; never let one through, whatever the caller did.
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return Err("invalid process id".into());
    };
    if raw <= 1 {
        return Err("refusing to signal a process group or init".into());
    }
    // SAFETY: kill(2) with a positive, validated pid.
    let rc = unsafe { libc::kill(raw, sig) };
    if rc == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(err.to_string())
        }
    }
}

/// Force-stops `pid` only if it is still the process that started at `start`.
/// Gone or reused pids are left alone and count as success.
#[cfg(unix)]
fn force_kill(pid: u32, start: u64) -> Result<(), String> {
    if is_system_pid(pid) {
        return Err("refusing to stop a system process".into());
    }
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[p]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    match sys.process(p) {
        Some(proc_) if proc_.start_time() == start && !is_zombie(proc_) => {
            send_signal(pid, libc::SIGKILL)
        }
        _ => Ok(()),
    }
}

/// Force-stops `pid` only if it is still the process that started at `start`.
/// The start time is read from the same handle that terminates it, and an
/// open handle keeps the process object alive, so the pid can't be reused
/// between the check and the kill. Gone or reused pids count as success.
#[cfg(windows)]
fn force_kill(pid: u32, start: u64) -> Result<(), String> {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        TerminateProcess,
    };
    use windows::core::HRESULT;
    if is_system_pid(pid) {
        return Err("refusing to stop a system process".into());
    }
    // SAFETY: plain Win32 calls on a handle we own; it is closed on every
    // path after a successful open, and the FILETIMEs outlive the call.
    unsafe {
        let h = match OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) {
            Ok(h) => h,
            // No such process: already gone.
            Err(e) if e.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => return Ok(()),
            Err(_) => return Err("Windows refused to stop it".into()),
        };
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let result = match GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user) {
            Err(_) => Err("Can't verify it, skipped".to_string()),
            Ok(()) if filetime_secs(created) != start => Ok(()), // reused pid
            Ok(()) => TerminateProcess(h, 1).map_err(|_| "Windows refused to stop it".to_string()),
        };
        let _ = CloseHandle(h);
        result
    }
}

/// Seconds since the Unix epoch, computed the way sysinfo does for
/// `start_time`, so the two compare exactly.
#[cfg(windows)]
fn filetime_secs(ft: windows::Win32::Foundation::FILETIME) -> u64 {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    (ticks / 10_000_000).saturating_sub(11_644_473_600)
}

#[cfg(not(any(unix, windows)))]
fn force_kill(_pid: u32, _start: u64) -> Result<(), String> {
    Err("can't force close on this system".into())
}

/// Which of `live` are still running and still the same processes.
fn still_running(live: &[Live]) -> Vec<u32> {
    let pids: Vec<Pid> = live.iter().map(|l| Pid::from_u32(l.pid)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    live.iter()
        .filter(|l| {
            sys.process(Pid::from_u32(l.pid))
                .is_some_and(|p| p.start_time() == l.start && !is_zombie(p))
        })
        .map(|l| l.pid)
        .collect()
}

fn is_zombie(p: &sysinfo::Process) -> bool {
    matches!(
        p.status(),
        sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
    )
}

fn wait_for_exit(live: &[Live], timeout: Duration) -> Vec<u32> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = still_running(live);
        if left.is_empty() || Instant::now() >= deadline {
            return left;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn freed(live: &[Live], left: &[u32]) -> u64 {
    live.iter()
        .filter(|l| !left.contains(&l.pid))
        .map(|l| l.memory)
        .sum()
}

pub fn close(target: &ProcTarget, force: bool, guard: &Guard) -> Outcome {
    let live = match validate(target, guard) {
        Ok(l) => l,
        Err(message) => return Outcome::Refused { message },
    };
    if live.is_empty() {
        return Outcome::Closed {
            message: "Already closed".to_string(),
            freed: 0,
            disk: 0,
        };
    }
    let pids: Vec<u32> = live.iter().map(|l| l.pid).collect();

    if force {
        // Also whatever the group started since the scan, each re-checked.
        let mut live = live;
        match live_descendants(&live, guard) {
            Ok(extra) => live.extend(extra),
            Err(message) => return Outcome::Refused { message },
        }
        let mut errors = Vec::new();
        // Deepest first, so a parent can't respawn them on its way out.
        for l in live.iter().rev() {
            if let Err(e) = force_kill(l.pid, l.start) {
                errors.push(e);
            }
        }
        let left = wait_for_exit(&live, Duration::from_secs(3));
        return if left.is_empty() {
            let bytes = freed(&live, &left);
            Outcome::Closed {
                message: format!("Freed {}", crate::engine::assess::fmt_bytes(bytes)),
                freed: bytes,
                disk: 0,
            }
        } else {
            Outcome::Failed {
                message: errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Still running".to_string()),
            }
        };
    }

    // Polite: the app's own Quit, or SIGTERM to each command-line process.
    let asked = match target.app_pid.filter(|a| pids.contains(a)) {
        Some(app_pid) => platform::ask_app_to_quit(app_pid, &pids),
        None => {
            if !platform::can_ask_politely(None, &pids) {
                return Outcome::NeedsForce {
                    message: "No window, force close only".to_string(),
                };
            }
            pids.iter()
                .try_for_each(|pid| platform::ask_process_to_stop(*pid))
        }
    };
    if let Err(message) = asked {
        return Outcome::NeedsForce { message };
    }
    let wait = if target.app_pid.is_some() {
        Duration::from_secs(8)
    } else {
        Duration::from_secs(5)
    };
    let left = wait_for_exit(&live, wait);
    if left.is_empty() {
        let bytes = freed(&live, &left);
        Outcome::Closed {
            message: format!("Freed {}", crate::engine::assess::fmt_bytes(bytes)),
            freed: bytes,
            disk: 0,
        }
    } else {
        Outcome::StillRunning {
            message: if target.app_pid.is_some() {
                "Still open, may be asking to save".to_string()
            } else {
                "Didn't stop".to_string()
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    /// Starts a real, windowless child that keeps running for minutes.
    pub(crate) fn spawn_sleeper() -> (Child, u64) {
        #[cfg(unix)]
        let mut cmd = Command::new("sleep");
        #[cfg(unix)]
        cmd.arg("300");
        #[cfg(windows)]
        let mut cmd = Command::new("ping");
        #[cfg(windows)]
        cmd.args(["-n", "300", "127.0.0.1"]);
        let child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleeper");
        let mut sys = System::new();
        let pid = Pid::from_u32(child.id());
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        let start = sys.process(pid).expect("sleeper visible").start_time();
        (child, start)
    }

    /// Whether `pid` is running (zombies count as gone).
    pub(crate) fn alive(pid: u32) -> bool {
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            true,
            ProcessRefreshKind::nothing(),
        );
        sys.process(Pid::from_u32(pid))
            .is_some_and(|p| !is_zombie(p))
    }

    fn target(pid: u32, start: u64) -> ProcTarget {
        ProcTarget {
            title: "sleeper".into(),
            members: vec![(pid, start)],
            app_pid: None,
            memory: 0,
        }
    }

    fn reap(mut c: Child) {
        let _ = c.kill();
        let _ = c.wait();
    }

    #[cfg(windows)]
    #[test]
    fn windowless_console_process_needs_force_and_keeps_running() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        let guard = Guard {
            forbidden: [0, 1, std::process::id()].into_iter().collect(),
        };
        let out = close(&target(pid, start), false, &guard);
        assert!(matches!(out, Outcome::NeedsForce { .. }), "{out:?}");
        assert!(alive(pid), "a polite request must not stop it");
        reap(child);
    }

    #[test]
    fn forced_close_stops_a_real_process() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        let guard = Guard {
            forbidden: [0, 1, std::process::id()].into_iter().collect(),
        };
        let reaper = std::thread::spawn(move || {
            let mut c = child;
            let _ = c.wait();
        });
        let out = close(&target(pid, start), true, &guard);
        assert!(matches!(out, Outcome::Closed { .. }), "{out:?}");
        reaper.join().unwrap();
        assert!(!alive(pid));
    }

    #[cfg(unix)]
    #[test]
    fn polite_close_stops_a_real_process() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        let guard = Guard {
            forbidden: [0, 1, std::process::id()].into_iter().collect(),
        };
        // Reap in the background so the exited child doesn't linger as a zombie.
        let reaper = std::thread::spawn(move || {
            let mut c = child;
            let _ = c.wait();
        });
        let out = close(&target(pid, start), false, &guard);
        assert!(matches!(out, Outcome::Closed { .. }), "{out:?}");
        reaper.join().unwrap();
    }

    #[test]
    fn reused_pid_is_never_signalled() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        let guard = Guard {
            forbidden: [0, 1].into_iter().collect(),
        };
        // Pretend the scan saw a different process with this pid.
        let out = close(&target(pid, start + 1), true, &guard);
        assert!(
            matches!(out, Outcome::Closed { .. }),
            "treated as already gone: {out:?}"
        );
        // The real process must still be alive.
        assert!(alive(pid));
        reap(child);
    }

    #[test]
    fn forbidden_pids_are_refused() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        let guard = Guard {
            forbidden: [0, 1, pid].into_iter().collect(),
        };
        let out = close(&target(pid, start), true, &guard);
        assert!(matches!(out, Outcome::Refused { .. }), "{out:?}");
        assert!(alive(pid));
        reap(child);
    }

    #[test]
    fn init_pid_is_refused_even_without_a_guard() {
        let guard = Guard {
            forbidden: HashSet::new(),
        };
        let out = close(&target(1, 0), true, &guard);
        assert!(matches!(out, Outcome::Refused { .. }), "{out:?}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_system_pids_are_never_force_stopped() {
        // 0 is the idle process, 4 is System.
        for pid in 0..=4 {
            assert!(force_kill(pid, 0).is_err(), "pid {pid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn init_and_group_pids_are_never_signalled() {
        assert!(send_signal(0, libc::SIGTERM).is_err());
        assert!(send_signal(1, libc::SIGTERM).is_err());
        assert!(
            send_signal(u32::MAX, libc::SIGTERM).is_err(),
            "would be -1: every process"
        );
    }

    #[test]
    fn guard_includes_self_and_ancestors() {
        let g = Guard::current(None);
        assert!(g.forbidden.contains(&std::process::id()));
        assert!(g.forbidden.contains(&0) && g.forbidden.contains(&1));
        assert!(g.forbidden.len() >= 4, "self, at least one parent, 0 and 1");
    }

    #[test]
    fn guard_includes_the_frontmost_app() {
        assert!(Guard::current(Some(987_654)).forbidden.contains(&987_654));
    }

    #[test]
    fn descendants_follow_verified_edges_level_by_level() {
        let table: Vec<Row> = vec![
            (10, None, 100),
            (12, Some(11), 105),
            (11, Some(10), 100), // same second as its parent: allowed
            (13, Some(10), 101),
            (99, Some(50), 100), // unrelated tree
        ];
        assert_eq!(
            descendants(&table, &[(10, 100)]),
            vec![(11, 100), (13, 101), (12, 105)],
            "root-first, so close() reverses it to deepest-first"
        );
    }

    #[test]
    fn descendants_never_cross_a_reused_or_unknown_edge() {
        let table: Vec<Row> = vec![
            (10, None, 100),
            (11, Some(10), 90), // started before its "parent": pid 10 was reused
            (12, Some(10), 0),  // unknown start
            (13, Some(12), 120),
        ];
        assert!(descendants(&table, &[(10, 100)]).is_empty());
        // The root itself was reused since the scan: walk nothing.
        let table: Vec<Row> = vec![(10, None, 200), (11, Some(10), 210)];
        assert!(descendants(&table, &[(10, 100)]).is_empty());
        // Unknown root start: walk nothing.
        let table: Vec<Row> = vec![(10, None, 0), (11, Some(10), 210)];
        assert!(descendants(&table, &[(10, 0)]).is_empty());
    }

    #[test]
    fn descendants_survive_cycles_and_skip_members() {
        let table: Vec<Row> = vec![
            (10, Some(11), 100),
            (11, Some(10), 100),
            (12, Some(11), 101),
        ];
        assert_eq!(
            descendants(&table, &[(10, 100)]),
            vec![(11, 100), (12, 101)]
        );
        assert_eq!(
            descendants(&table, &[(10, 100), (11, 100)]),
            vec![(12, 101)],
            "members are never repeated"
        );
        let self_parent: Vec<Row> = vec![(10, Some(10), 100)];
        assert!(descendants(&self_parent, &[(10, 100)]).is_empty());
    }

    /// Starts a shell that runs a sleeper as its own child (not exec'd), and
    /// waits for that child to appear. Returns (shell, shell start, child pid,
    /// child start).
    fn spawn_shell_with_child() -> (Child, u64, u32, u64) {
        #[cfg(unix)]
        let mut cmd = Command::new("sh");
        #[cfg(unix)]
        cmd.args(["-c", "sleep 300; :"]);
        #[cfg(windows)]
        let mut cmd = Command::new("cmd");
        #[cfg(windows)]
        cmd.args(["/c", "ping -n 300 127.0.0.1 >nul"]);
        let shell = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn shell");
        let shell_pid = Pid::from_u32(shell.id());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut sys = System::new();
        let found = loop {
            sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().without_tasks(),
            );
            let shell_start = sys.process(shell_pid).map(|p| p.start_time());
            let child = sys
                .processes()
                .values()
                .find(|p| p.parent() == Some(shell_pid) && !is_zombie(p));
            if let (Some(s), Some(c)) = (shell_start, child) {
                break Some((s, c.pid().as_u32(), c.start_time()));
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        match found {
            Some((shell_start, child, child_start)) => (shell, shell_start, child, child_start),
            None => {
                reap(shell);
                panic!("shell never started its child");
            }
        }
    }

    #[test]
    fn forced_close_stops_children_started_after_the_scan() {
        let (shell, shell_start, child, _) = spawn_shell_with_child();
        let shell_pid = shell.id();
        let guard = Guard {
            forbidden: [0, 1, std::process::id()].into_iter().collect(),
        };
        let reaper = std::thread::spawn(move || {
            let mut s = shell;
            let _ = s.wait();
        });
        // The scan only saw the shell.
        let out = close(&target(shell_pid, shell_start), true, &guard);
        assert!(matches!(out, Outcome::Closed { .. }), "{out:?}");
        reaper.join().unwrap();
        assert!(!alive(shell_pid));
        assert!(!alive(child), "the child must not survive");
    }

    #[test]
    fn forbidden_descendant_refuses_the_whole_close() {
        let (shell, shell_start, child, child_start) = spawn_shell_with_child();
        let shell_pid = shell.id();
        let guard = Guard {
            forbidden: [0, 1, std::process::id(), child].into_iter().collect(),
        };
        let out = close(&target(shell_pid, shell_start), true, &guard);
        assert!(matches!(out, Outcome::Refused { .. }), "{out:?}");
        assert!(alive(shell_pid), "nothing in a mixed tree is signalled");
        assert!(alive(child));
        let _ = force_kill(child, child_start);
        reap(shell);
    }

    #[test]
    fn force_kill_with_wrong_start_is_never_signalled() {
        let (child, start) = spawn_sleeper();
        let pid = child.id();
        assert_eq!(force_kill(pid, start + 1), Ok(()), "treated as gone");
        assert!(alive(pid));
        reap(child);
    }
}
