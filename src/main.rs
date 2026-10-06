mod ui;

use anyhow::{Context, Result, anyhow};
use cracker_desktop::Storage;
use directories::ProjectDirs;
use gpui_kit::{App, AppContext, Bounds, KeyBinding, WindowBounds, WindowOptions, px, size};
use std::{env, fs, path::PathBuf, process::ExitCode};
use tracing_subscriber::EnvFilter;
use ui::TrackerApp;

gpui_kit::actions!(cracker, [Quit]);

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "Application startup failed");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let database_path = database_path()?;
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).context("creating application data directory")?;
    }
    let storage = Storage::open(&database_path)?;
    let snapshot = storage.snapshot()?;
    tracing::info!(path = %database_path.display(), "Local database ready");

    // Application::run owns the macOS event loop; retain startup errors outside it.
    let startup_error = std::rc::Rc::new(std::cell::RefCell::new(None));
    let window_error = startup_error.clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(760.0), px(520.0))),
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("Cracker — Finance Tracker".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, move |_, cx| {
                cx.new(|_| TrackerApp::new(storage, snapshot, database_path))
            }) {
                *window_error.borrow_mut() = Some(error);
                cx.quit();
            } else {
                cx.activate(true);
            }
        });

    if let Some(error) = startup_error.borrow_mut().take() {
        return Err(error.context("opening GPUI window"));
    }
    Ok(())
}

fn database_path() -> Result<PathBuf> {
    if let Some(directory) = env::var_os("CRACKER_DATA_DIR") {
        if directory.is_empty() {
            return Err(anyhow!("CRACKER_DATA_DIR must not be empty"));
        }
        return Ok(PathBuf::from(directory).join("finance.sqlite3"));
    }
    let directories = ProjectDirs::from("", "", "Cracker")
        .context("could not locate the user application data directory")?;
    Ok(directories.data_dir().join("finance.sqlite3"))
}
