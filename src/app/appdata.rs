use std::path::PathBuf;

#[derive(Debug)]
pub struct AppData {
    pub config_dir: PathBuf,
    pub icons_dir: PathBuf,
    pub state_dir: PathBuf,
    pub lock_file: PathBuf,
    pub socket_file: PathBuf,
}

impl AppData {
    pub fn new() -> anyhow::Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        let config_dir = home.join(".config").join("general-sys-tray-app");
        let icons_dir = config_dir.join("icons");
        let state_dir = config_dir.join("state");
        let lock_file = config_dir.join("app.lock");
        let socket_file = runtime_dir()?.join("general-sys-tray-app.sock");

        std::fs::create_dir_all(&icons_dir)?;
        std::fs::create_dir_all(&state_dir)?;

        Ok(Self {
            config_dir,
            icons_dir,
            state_dir,
            lock_file,
            socket_file,
        })
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.yaml")
    }

    pub fn icon_path(&self, name: &str) -> PathBuf {
        self.icons_dir.join(name)
    }

    pub fn target_state_path(&self, target_name: &str) -> PathBuf {
        self.state_dir.join(format!("{}.json", target_name))
    }
}

fn runtime_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    if let Some(dir) = std::env::var_os("TMPDIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from("/tmp"))
}
