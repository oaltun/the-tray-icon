use crate::app::appdata::AppData;

pub fn ensure_autostart(_appdata: &AppData, autostart: bool) -> anyhow::Result<()> {
    let autostart_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join(".config")
        .join("autostart");
    std::fs::create_dir_all(&autostart_dir)?;

    let desktop_file = autostart_dir.join("general-sys-tray-app.desktop");

    if autostart {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "/usr/local/bin/general-sys-tray-app".to_string());

        let contents = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Exec={}\n\
             Hidden=false\n\
             NoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\n",
            exe
        );

        std::fs::write(&desktop_file, contents)?;
    } else if desktop_file.exists() {
        std::fs::remove_file(&desktop_file)?;
    }

    Ok(())
}
