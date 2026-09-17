use std::sync::Arc;
use std::time::Duration;

use tray_icon::app::appdata::AppData;
use tray_icon::app::autostart;
use tray_icon::app::config::Config;
use tray_icon::app::monitor::{start_monitor, MonitorCommand};
use tray_icon::app::single_instance::try_acquire_lock;
use clap::Parser;

/// System tray app that monitors background jobs via per-target scripts.
#[derive(Parser, Debug)]
#[command(name = "general-sys-tray-app", author, version, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    #[arg(long)]
    poll_now: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.poll_now {
        return poll_now();
    }

    let config_path = args
        .config
        .or_else(|| std::env::var_os("GSTA_CONFIG").map(std::path::PathBuf::from))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".config")
                .join("general-sys-tray-app")
                .join("config.yaml")
        });

    let config_data = std::fs::read_to_string(&config_path)
        .or_else(|_| std::fs::read_to_string(
            dirs::home_dir()
                .unwrap_or_default()
                .join(".config")
                .join("general-sys-tray-app")
                .join("config.yaml"),
        ))
        .map_err(|_| anyhow::anyhow!("config file not found at {}", config_path.display()))?;

    let config: Config = serde_yaml::from_str(&config_data)
        .map_err(|e| anyhow::anyhow!("failed to parse config: {}", e))?;

    config.validate()?;

    let appdata = AppData::new()?;

    if config.autostart {
        let _ = autostart::ensure_autostart(&appdata, true);
    }

    let lock = try_acquire_lock(&appdata)?;
    if let Some(ref f) = lock {
        let _ = tray_icon::app::single_instance::write_lock_pid(f);
    }

    let config = Arc::new(config);

    run(config, appdata)
}

#[cfg(target_os = "linux")]
fn run(config: Arc<Config>, appdata: AppData) -> anyhow::Result<()> {
    use tray_icon::app::ui::TrayApp;

    let (update_tx, update_rx) = crossbeam_channel::unbounded();
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

    let monitor_handle = start_monitor(config.clone(), update_tx, cmd_rx);

    gtk::init().unwrap();

    let socket_path = appdata.socket_file.clone();
    let listener = {
        use std::os::unix::net::UnixListener;
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        listener
    };

    let mut app = TrayApp::new(config.clone(), appdata);
    app.init()?;

    let quit_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    {
        let quit_flag = quit_flag.clone();
        let cmd_tx = cmd_tx.clone();

        // Runs only on the main thread inside the GTK main loop, so
        // timeout_add_local (no Send bound) is required for the non-Send
        // TrayIcon inside TrayApp.
        glib::timeout_add_local(Duration::from_millis(50), move || {
            if quit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                gtk::main_quit();
                return glib::ControlFlow::Break;
            }

            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                handle_menu_event(&app, event, &cmd_tx);
            }

            while let Ok(update) = update_rx.try_recv() {
                app.handle_update(update);
            }

            while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
                handle_tray_event(&app, event, &cmd_tx);
            }

            while let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 16];
                let _ = stream.read(&mut buf);
                let _ = cmd_tx.send(MonitorCommand::PollNow);
            }

            glib::ControlFlow::Continue
        });
    }

    gtk::main();

    let _ = std::fs::remove_file(&socket_path);
    let _ = cmd_tx.send(MonitorCommand::Quit);
    let _ = monitor_handle.join();

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn run(_config: Arc<Config>, _appdata: AppData) -> anyhow::Result<()> {
    eprintln!("general-sys-tray-app is only supported on Linux");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn handle_menu_event(
    app: &tray_icon::app::ui::TrayApp,
    event: tray_icon::menu::MenuEvent,
    cmd_tx: &crossbeam_channel::Sender<MonitorCommand>,
) {
    use tray_icon::app::monitor::MonitorCommand;

    let menu_id = event.id().0.clone();

    for target in &app.config.targets {
        let label = target.label.clone().unwrap_or_else(|| target.name.clone());

        if menu_id.contains(&format!("Show tail: {}", label)) {
            show_tail_window(target, app);
        } else if menu_id == "Open log in viewer" && target.log.is_some() {
            if let Some(ref log) = target.log {
                let _ = open::that(log);
            }
        } else if menu_id == format!("Restart \"{}\"", label) && target.restart_script.is_some() {
            let _ = cmd_tx.send(MonitorCommand::Restart { target_name: target.name.clone() });
        } else if menu_id == format!("Stop \"{}\"", label) {
            let _ = cmd_tx.send(MonitorCommand::Stop { target_name: target.name.clone() });
        } else if menu_id == "Poll now" {
            let _ = cmd_tx.send(MonitorCommand::PollNow);
        } else if menu_id == "Preferences\u{2026}" {
            open_preferences(app);
        } else if menu_id == "Quit" {
            gtk::main_quit();
        }
    }
}

#[cfg(target_os = "linux")]
fn handle_tray_event(
    _app: &tray_icon::app::ui::TrayApp,
    event: tray_icon::TrayIconEvent,
    _cmd_tx: &crossbeam_channel::Sender<MonitorCommand>,
) {
    if let tray_icon::TrayIconEvent::Click { button, .. } = event {
        if button == tray_icon::MouseButton::Left {
            show_summary_window(_app);
        }
    }
}

#[cfg(target_os = "linux")]
fn show_tail_window(target: &tray_icon::app::config::TargetConfig, app: &tray_icon::app::ui::TrayApp) {
    use gtk::prelude::*;

    let state = app.targets.get(&target.name);
    let tail_text = state.map(|s| s.tail_text.clone()).unwrap_or_default();

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(&format!("Tail: {}", target.label.clone().unwrap_or_else(|| target.name.clone())));
    window.set_default_size(600, 400);
    window.set_keep_above(true);

    let scrolled = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_wrap_mode(gtk::WrapMode::Word);
    text_view.set_left_margin(10);
    text_view.set_right_margin(10);
    text_view.set_top_margin(10);
    text_view.set_bottom_margin(10);

    if let Some(buffer) = text_view.buffer() {
        buffer.set_text(&tail_text);
    }

    scrolled.add(&text_view);
    window.add(&scrolled);

    window.show_all();
    window.connect_delete_event(|_, _| glib::Propagation::Proceed);
}

#[cfg(target_os = "linux")]
fn show_summary_window(app: &tray_icon::app::ui::TrayApp) {
    use gtk::prelude::*;

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("general-sys-tray-app");
    window.set_default_size(500, 300);
    window.set_keep_above(true);

    let scrolled = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    let list_store = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING, glib::Type::STRING]);

    for target in &app.config.targets {
        let state = app.targets.get(&target.name);
        let status = state.map(|s| s.status.glyph().to_string()).unwrap_or_default();
        let label = target.label.clone().unwrap_or_else(|| target.name.clone());
        let tail = state.map(|s| s.tail_text.clone()).unwrap_or_default();
        list_store.insert_with_values(None, &[(0, &status), (1, &label), (2, &tail)]);
    }

    let tree_view = gtk::TreeView::with_model(&list_store);
    for (i, title) in ["Status", "Target", "Tail"].iter().enumerate() {
        let column = gtk::TreeViewColumn::new();
        let cell = gtk::CellRendererText::new();
        gtk::prelude::TreeViewColumnExt::pack_start(&column, &cell, true);
        gtk::prelude::TreeViewColumnExt::add_attribute(&column, &cell, "text", i as i32);
        column.set_title(title);
        tree_view.append_column(&column);
    }

    scrolled.add(&tree_view);
    window.add(&scrolled);

    window.show_all();
    window.connect_delete_event(|_, _| glib::Propagation::Proceed);
}

#[cfg(target_os = "linux")]
fn open_preferences(app: &tray_icon::app::ui::TrayApp) {
    let _ = open::that(&app.appdata.config_dir);
}

fn poll_now() -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let socket_path = AppData::new()?.socket_file;

    if !socket_path.exists() {
        anyhow::bail!(
            "no running instance found (socket {} is missing)",
            socket_path.display()
        );
    }

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| anyhow::anyhow!("failed to connect to running instance: {}", e))?;
    stream.write_all(b"poll\n")?;
    Ok(())
}
