use crate::app::appdata::AppData;
use crate::app::config::Config;
use crate::app::monitor::{MonitorUpdate, TargetState};
use crate::app::status::Status;
use std::collections::HashMap;

pub struct TrayApp {
    pub config: std::sync::Arc<Config>,
    pub appdata: AppData,
    pub targets: HashMap<String, TargetState>,
    pub tray_icon: Option<crate::TrayIcon>,
    /// Handles to the per-target header items, so status changes can be
    /// applied in place (set_text) instead of rebuilding the menu —
    /// re-attaching the menu resets any open menu popup.
    target_headers: HashMap<String, crate::menu::MenuItem>,
    /// Last text written to each header item; skip set_text when unchanged.
    header_texts: HashMap<String, String>,
    /// Last aggregate status pushed to the tray icon; skip set_icon when
    /// unchanged — re-setting the icon makes AppIndicator repaint it,
    /// which reads as flicker.
    last_agg: Option<Status>,
}

impl TrayApp {
    pub fn new(config: std::sync::Arc<Config>, appdata: AppData) -> Self {
        Self {
            config,
            appdata,
            targets: HashMap::new(),
            tray_icon: None,
            target_headers: HashMap::new(),
            header_texts: HashMap::new(),
            last_agg: None,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        let icon = crate::app::app_icon::fallback_icon_for(Status::Ok);
        let tooltip = "general-sys-tray-app".to_string();

        let menu = self.build_menu();

        let tray = crate::TrayIconBuilder::new()
            .with_tooltip(tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()?;

        self.tray_icon = Some(tray);

        Ok(())
    }

    pub fn build_menu(&mut self) -> crate::menu::Menu {
        let menu = crate::menu::Menu::new();
        self.target_headers.clear();
        self.header_texts.clear();

        let targets_submenu = crate::menu::Submenu::new("Targets", true);
        for target in &self.config.targets {
            let status = self.targets.get(&target.name).map(|s| s.status).unwrap_or(Status::Ok);
            let label = target.label.clone().unwrap_or_else(|| target.name.clone());
            let header = format!("{} {}", status.glyph(), label);

            let header_item = crate::menu::MenuItem::new(&header, false, None);
            targets_submenu.append(&header_item).ok();
            self.target_headers.insert(target.name.clone(), header_item);
            self.header_texts.insert(target.name.clone(), header);

            let show_tail = crate::menu::MenuItem::new(
                format!("  Show tail: {}", label),
                true,
                None,
            );
            targets_submenu.append(&show_tail).ok();

            let open_log = crate::menu::MenuItem::new(
                "  Open log in viewer",
                target.log.is_some(),
                None,
            );
            targets_submenu.append(&open_log).ok();

            let restart = crate::menu::MenuItem::new(
                format!("  Restart \"{}\"", label),
                target.restart_script.is_some(),
                None,
            );
            targets_submenu.append(&restart).ok();

            let stop = crate::menu::MenuItem::new(
                format!("  Stop \"{}\"", label),
                true,
                None,
            );
            targets_submenu.append(&stop).ok();
        }

        menu.append(&targets_submenu).ok();

        let separator = crate::menu::PredefinedMenuItem::separator();
        menu.append(&separator).ok();

        let poll_now = crate::menu::MenuItem::new("Poll now", true, None);
        menu.append(&poll_now).ok();

        let preferences = crate::menu::MenuItem::new("Preferences\u{2026}", true, None);
        menu.append(&preferences).ok();

        let quit = crate::menu::MenuItem::new("Quit", true, None);
        menu.append(&quit).ok();

        menu
    }

    pub fn handle_update(&mut self, update: MonitorUpdate) {
        match update {
            MonitorUpdate::TargetUpdate { name, status, tail_text, pid, last_modified } => {
                if name.is_empty() {
                    return;
                }
                let label = self.config.targets.iter()
                    .find(|t| t.name == name)
                    .and_then(|t| t.label.clone())
                    .unwrap_or_else(|| name.clone());

                self.targets.insert(name.clone(), TargetState {
                    name,
                    label,
                    status,
                    tail_text,
                    last_checked: std::time::SystemTime::now(),
                    last_modified,
                    pid,
                });

                self.update_icon();
                self.update_menu_labels();
            }
        }
    }

    fn update_icon(&mut self) {
        let statuses: HashMap<String, Status> = self
            .targets
            .iter()
            .map(|(k, v)| (k.clone(), v.status))
            .collect();

        let agg = crate::app::app_icon::aggregate_status(&statuses);
        if self.last_agg == Some(agg) {
            return;
        }
        self.last_agg = Some(agg);

        let icon = crate::app::app_icon::fallback_icon_for(agg);
        if let Some(ref tray) = self.tray_icon {
            let _ = tray.set_icon(Some(icon));
        }
    }

    /// Updates the per-target header labels in place; only items whose
    /// status actually changed get set_text, so an open menu keeps its
    /// position and unaffected items are not redrawn.
    fn update_menu_labels(&mut self) {
        for (name, item) in &self.target_headers {
            let status = self.targets.get(name).map(|s| s.status).unwrap_or(Status::Ok);
            let label = self
                .config
                .targets
                .iter()
                .find(|t| &t.name == name)
                .and_then(|t| t.label.clone())
                .unwrap_or_else(|| name.clone());
            let text = format!("{} {}", status.glyph(), label);

            if self.header_texts.get(name) != Some(&text) {
                item.set_text(&text);
                self.header_texts.insert(name.clone(), text);
            }
        }
    }
}
