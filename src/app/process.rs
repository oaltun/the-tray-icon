use crate::app::config::TargetConfig;

pub fn resolve_pid(target: &TargetConfig) -> Option<u32> {
    if let Some(ref pid_file) = target.pid_file {
        if let Ok(content) = std::fs::read_to_string(pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if process_exists(pid) {
                    return Some(pid);
                }
            }
        }
    }

    if let Some(ref pattern) = target.pid_pattern {
        if let Ok(pid) = find_pid_by_pattern(pattern) {
            return Some(pid);
        }
    }

    None
}

pub fn stop_target(config: &crate::app::config::Config, target_name: &str) -> anyhow::Result<()> {
    let target = config
        .targets
        .iter()
        .find(|t| t.name == target_name)
        .ok_or_else(|| anyhow::anyhow!("target not found: {}", target_name))?;

    let pid = resolve_pid(target).ok_or_else(|| anyhow::anyhow!("pid not found for {}", target_name))?;

    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }

    let wait_secs = target.stoppage_after_secs.unwrap_or(10);
    std::thread::sleep(std::time::Duration::from_secs(wait_secs.min(30)));

    if process_exists(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}

pub fn restart_target(config: &crate::app::config::Config, target_name: &str) -> anyhow::Result<()> {
    let _ = stop_target(config, target_name);

    let target = config
        .targets
        .iter()
        .find(|t| t.name == target_name)
        .ok_or_else(|| anyhow::anyhow!("target not found: {}", target_name))?;

    if let Some(ref script) = target.restart_script {
        let nice_val = config.default_nice;
        let ionice_class = match config.default_ionice_class.as_str() {
            "idle" => "3",
            "best-effort" => "2",
            "realtime" => "1",
            other => {
                eprintln!("warning: unknown ionice class '{}', using idle", other);
                "3"
            }
        };

        let mut cmd = std::process::Command::new("ionice");
        cmd.arg("-c").arg(ionice_class);
        cmd.arg("nice");
        cmd.arg("-n").arg(nice_val.to_string());
        cmd.arg(script);

        if let Some(log) = &target.log {
            cmd.arg("--log").arg(log);
        }

        cmd.spawn()
            .map_err(|e| anyhow::anyhow!("failed to start restart_script: {}", e))?;
    }

    Ok(())
}

fn process_exists(pid: u32) -> bool {
    let path = format!("/proc/{}", pid);
    std::path::Path::new(&path).exists()
}

fn find_pid_by_pattern(pattern: &str) -> anyhow::Result<u32> {
    let output = std::process::Command::new("ps")
        .args(["aux"])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run ps: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains(pattern) && !line.contains("grep") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(pid_str) = parts.get(1) {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    return Ok(pid);
                }
            }
        }
    }

    anyhow::bail!("no process matching pattern: {}", pattern)
}
