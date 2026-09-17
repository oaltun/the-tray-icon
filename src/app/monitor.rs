use crate::app::status::Status;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub struct TargetState {
    pub name: String,
    pub label: String,
    pub status: Status,
    pub tail_text: String,
    pub last_checked: SystemTime,
    pub last_modified: Option<SystemTime>,
    pub pid: Option<u32>,
}

#[derive(Debug)]
pub enum MonitorUpdate {
    TargetUpdate {
        name: String,
        status: Status,
        tail_text: String,
        pid: Option<u32>,
        last_modified: Option<SystemTime>,
    },
}

#[derive(Debug)]
pub enum MonitorCommand {
    PollNow,
    Quit,
    Stop { target_name: String },
    Restart { target_name: String },
}

pub fn start_monitor(
    config: std::sync::Arc<crate::app::config::Config>,
    update_tx: crossbeam_channel::Sender<MonitorUpdate>,
    cmd_rx: crossbeam_channel::Receiver<MonitorCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut states: HashMap<String, TargetState> = HashMap::new();

        for target in &config.targets {
            states.insert(
                target.name.clone(),
                TargetState {
                    name: target.name.clone(),
                    label: target.label.clone().unwrap_or_else(|| target.name.clone()),
                    status: Status::Ok,
                    tail_text: String::new(),
                    last_checked: SystemTime::now(),
                    last_modified: None,
                    pid: None,
                },
            );
        }

        loop {
            if let Ok(MonitorCommand::Quit) = cmd_rx.try_recv() {
                break;
            }

            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    MonitorCommand::Quit => {
                        let _ = update_tx.send(MonitorUpdate::TargetUpdate {
                            name: String::new(),
                            status: Status::Ok,
                            tail_text: String::new(),
                            pid: None,
                            last_modified: None,
                        });
                        std::process::exit(0);
                    }
                    MonitorCommand::PollNow => {}
                    MonitorCommand::Stop { target_name } => {
                        let _ = crate::app::process::stop_target(&config, &target_name);
                    }
                    MonitorCommand::Restart { target_name } => {
                        let _ = crate::app::process::restart_target(&config, &target_name);
                    }
                }
            }

            for target in &config.targets {
                let result = check_target(&config, target);
                if let Some(state) = states.get_mut(&target.name) {
                    let tail_text = result.tail_text.clone();
                    state.status = result.status;
                    state.tail_text = tail_text;
                    state.last_checked = SystemTime::now();
                    state.last_modified = result.last_modified;
                    state.pid = result.pid;

                    let _ = update_tx.send(MonitorUpdate::TargetUpdate {
                        name: target.name.clone(),
                        status: result.status,
                        tail_text: result.tail_text.clone(),
                        pid: result.pid,
                        last_modified: result.last_modified,
                    });
                }
            }

            match cmd_rx.recv_timeout(Duration::from_secs(config.poll_interval_secs)) {
                Ok(MonitorCommand::PollNow) => continue,
                Ok(MonitorCommand::Quit) => break,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
    })
}

struct CheckResult {
    status: Status,
    tail_text: String,
    last_modified: Option<SystemTime>,
    pid: Option<u32>,
}

fn check_target(config: &crate::app::config::Config, target: &crate::app::config::TargetConfig) -> CheckResult {
    let mut status = Status::Ok;
    let mut tail_text = String::new();
    let mut last_modified = None;
    let mut pid = None;

    if let Some(ref log_path) = target.log {
        if let Ok(metadata) = std::fs::metadata(log_path) {
            if let Ok(mtime) = metadata.modified() {
                last_modified = Some(mtime);
                let now = SystemTime::now();
                let stoppage = target
                    .stoppage_after_secs
                    .unwrap_or(600);
                if now.duration_since(mtime).unwrap_or_default() > Duration::from_secs(stoppage)
                    && status == Status::Ok
                {
                    status = Status::Warn;
                    tail_text = format!("log unchanged for {}s", stoppage);
                }
            }
        } else {
            status = Status::Err;
            tail_text = format!("log file not found: {}", log_path);
        }
    }

    if let Some(ref script_path) = target.script {
        match run_script(script_path, config.poll_interval_secs * 2) {
            Ok((script_status, script_tail)) => {
                status = script_status;
                tail_text = script_tail;
            }
            Err(e) => {
                status = Status::Err;
                tail_text = format!("script error: {}", e);
            }
        }
    }

    if target.pid_file.is_some() || target.pid_pattern.is_some() {
        pid = crate::app::process::resolve_pid(target);
        if pid.is_none() && status == Status::Ok {
            status = Status::Warn;
            tail_text = "pid not found".to_string();
        }
    }

    CheckResult {
        status,
        tail_text,
        last_modified,
        pid,
    }
}

fn run_script(script_path: &str, timeout_secs: u64) -> anyhow::Result<(Status, String)> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let path = script_path.to_string();
    std::thread::spawn(move || {
        let result = std::process::Command::new(&path).output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let mut tail_lines: Vec<&str> = stdout.lines().collect();
            let mut status = if output.status.success() {
                Status::Ok
            } else {
                Status::Err
            };

            if let Some(first) = tail_lines.first() {
                if let Some(header_status) = Status::from_header(first) {
                    status = header_status;
                    tail_lines.remove(0);
                }
            }

            let tail_text = if tail_lines.is_empty() {
                if !stderr.is_empty() {
                    stderr.lines().take(50).collect::<Vec<_>>().join("\n")
                } else {
                    String::new()
                }
            } else {
                tail_lines.iter().take(50).cloned().collect::<Vec<_>>().join("\n")
            };

            Ok((status, tail_text))
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("failed to run script: {}", e)),
        Err(_) => Ok((Status::Err, "(timed out)".to_string())),
    }
}
