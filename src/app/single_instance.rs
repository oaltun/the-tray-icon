use crate::app::appdata::AppData;

pub fn try_acquire_lock(appdata: &AppData) -> anyhow::Result<Option<std::fs::File>> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&appdata.lock_file)
    {
        Ok(file) => Ok(Some(file)),
        Err(_) => {
            // Check if the process holding the lock is still alive
            if let Ok(content) = std::fs::read_to_string(&appdata.lock_file) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if is_process_alive(pid) {
                        anyhow::bail!("another instance is already running (pid {})", pid);
                    } else {
                        // Stale lock, remove it
                        let _ = std::fs::remove_file(&appdata.lock_file);
                        return try_acquire_lock(appdata);
                    }
                }
            }
            // Corrupt lock, remove and retry
            let _ = std::fs::remove_file(&appdata.lock_file);
            try_acquire_lock(appdata)
        }
    }
}

pub fn write_lock_pid(file: &std::fs::File) -> anyhow::Result<()> {
    use std::io::Write;
    let pid = std::process::id().to_string();
    let mut f = file;
    f.write_all(pid.as_bytes())?;
    f.sync_all()?;
    Ok(())
}

fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}
