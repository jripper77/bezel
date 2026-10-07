//! Windows tray runtime, without a WebView. Rendering runs off the UI thread.
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use bezel_cli::{Cli, Command};
use bezel_power::handoff;
use clap::Parser;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};

enum Action {
    Menu(MenuEvent),
    Tray(TrayIconEvent),
}

// After editing, follow the newly saved Studio theme and screen. A standalone
// tray command without Studio settings keeps its original arguments.
fn follow_saved_theme(cli: &mut Cli) {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return;
    };
    let path = Path::new(&appdata).join("io.github.slipalison.bezel/settings.json");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(settings) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let Command::Run {
        theme,
        target,
        ffmpeg,
        settings: sensor_settings,
        ..
    } = &mut cli.command
    else {
        return;
    };
    if let Some(saved) = settings["lastTheme"].as_str()
        && Path::new(saved).is_file()
    {
        *theme = saved.into();
    }
    if let Some(screen) = settings["liveScreen"].as_str() {
        target.screen = Some(screen.into());
    }
    *ffmpeg = settings["ffmpegPath"].as_str().map(Into::into);
    sensor_settings.ping_host = settings["pingHost"].as_str().map(Into::into);
    sensor_settings.mangohud_dir = settings["mangohudDir"].as_str().map(Into::into);
}

pub(super) fn run(arguments: Vec<OsString>) -> anyhow::Result<String> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .context("no executable directory")?
        .to_path_buf();
    let runtime_directory = handoff::runtime_directory(&executable)?;
    let Some(_instance) = handoff::light_instance(&runtime_directory)? else {
        return Ok(String::new());
    };
    let quit_file = runtime_directory.join("bezel-light-quit");
    // Only the instance owning the lock clears requests left from earlier runs.
    if quit_file.exists() {
        std::fs::remove_file(&quit_file)?;
    }
    let initial = Cli::try_parse_from(&arguments)?;
    let finite = matches!(
        initial.command,
        Command::Run {
            frames: Some(_),
            ..
        }
    );
    let simulate = initial.fake;
    let mut event_loop = EventLoopBuilder::<Action>::with_user_event().build();
    let menu = Menu::new();
    let italian = sys_locale::get_locale().is_some_and(|locale| locale.starts_with("it"));
    let open = MenuItem::new(
        if italian {
            "Apri Studio"
        } else {
            "Open Studio"
        },
        true,
        None,
    );
    let quit = MenuItem::new(if italian { "Esci" } else { "Quit" }, true, None);
    menu.append_items(&[&open, &quit])?;
    let pixels = image::load_from_memory(include_bytes!(
        "../../../apps/bezel-studio/src-tauri/icons/tray.png"
    ))?
    .into_rgba8();
    let (width, height) = pixels.dimensions();
    let icon = Icon::from_rgba(pixels.into_raw(), width, height)?;
    let tray = TrayIconBuilder::new()
        .with_id("bezel-light")
        .with_tooltip("Bezel Light")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()?;
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(Action::Menu(event));
    }));
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(Action::Tray(event));
    }));
    let quit_requested = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit_requested);
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst))?;
    let mut worker: Option<std::thread::JoinHandle<anyhow::Result<String>>> = None;
    let mut stop = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut pending_open = false;
    let mut opening: Option<(std::process::Child, Instant)> = None;
    let mut visible = true;
    let mut retry_at = Instant::now();
    let mut reload_saved = false;
    let studio_exe = directory.join("bezel-studio.exe");
    let mut fatal = None;
    event_loop.run_return(|event, _, flow| {
        // Tao delivers LoopDestroyed synchronously inside WM_ENDSESSION,
        // then exits the process. Finish BEFORE returning from this callback.
        // The query/cancel phase does not deliver it and must leave Light live.
        if matches!(event, Event::LoopDestroyed) && bezel_power::session_ending() {
            shutdown.store(true, Ordering::SeqCst);
            stop.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_millis(3500);
            while worker.as_ref().is_some_and(|thread| !thread.is_finished())
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            if worker.as_ref().is_some_and(|thread| !thread.is_finished()) {
                eprintln!("bezel light: shutdown deadline reached; screen sleep timer remains the fallback");
            } else if let Some(thread) = worker.take() {
                match thread.join() {
                    Ok(Ok(summary)) => eprint!("{summary}"),
                    Ok(Err(error)) => eprintln!("bezel light: shutdown failed: {error:#}"),
                    Err(_) => eprintln!("bezel light: shutdown worker panicked"),
                }
            }
            return;
        }
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        let wants_open = match event {
            Event::UserEvent(Action::Menu(ref event)) if event.id == *quit.id() => {
                quit_requested.store(true, Ordering::SeqCst);
                false
            }
            Event::UserEvent(Action::Menu(ref event)) => event.id == *open.id(),
            Event::UserEvent(Action::Tray(TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            })) => true,
            _ => false,
        };
        if wants_open && studio_exe.is_file() {
            pending_open = true;
        }
        if quit_file.exists() {
            quit_requested.store(true, Ordering::SeqCst);
        }
        let studio_active = match handoff::studio_active(&runtime_directory) {
            Ok(active) => active,
            Err(error) => {
                fatal = Some(anyhow::Error::from(error));
                quit_requested.store(true, Ordering::SeqCst);
                true
            }
        };
        if studio_active {
            reload_saved = true;
        }
        if studio_active || pending_open || quit_requested.load(Ordering::SeqCst) {
            stop.store(true, Ordering::SeqCst);
        }
        if worker.as_ref().is_some_and(|thread| thread.is_finished()) {
            if let Some(thread) = worker.take() {
                match thread.join() {
                    Ok(Ok(summary)) => {
                        eprint!("{summary}");
                    }
                    Ok(Err(error)) => {
                        eprintln!("bezel light: {error:#}");
                    }
                    Err(_) => {
                        eprintln!("bezel light: rendering worker panicked");
                    }
                }
            }
            retry_at = Instant::now() + Duration::from_secs(3);
            if finite {
                quit_requested.store(true, Ordering::SeqCst);
            }
        }
        if pending_open && worker.is_none() && !quit_requested.load(Ordering::SeqCst) {
            match std::process::Command::new(&studio_exe)
                .current_dir(&directory)
                .spawn()
            {
                Ok(child) => opening = Some((child, Instant::now() + Duration::from_secs(30))),
                Err(error) => eprintln!("bezel light: Studio could not start: {error}"),
            }
            pending_open = false;
        }
        if let Some((child, deadline)) = opening.as_mut()
            && (studio_active
                || child.try_wait().ok().flatten().is_some()
                || Instant::now() >= *deadline)
        {
            opening = None;
        }
        if visible == studio_active {
            let _ = tray.set_visible(!studio_active);
            visible = !studio_active;
        }
        if quit_requested.load(Ordering::SeqCst) {
            if worker.is_none() {
                *flow = ControlFlow::Exit;
            }
        } else if !studio_active
            && !pending_open
            && opening.is_none()
            && worker.is_none()
            && Instant::now() >= retry_at
        {
            let mut cli = match Cli::try_parse_from(&arguments) {
                Ok(cli) => cli,
                Err(error) => {
                    fatal = Some(anyhow::Error::from(error));
                    quit_requested.store(true, Ordering::SeqCst);
                    return;
                }
            };
            if reload_saved && !simulate {
                follow_saved_theme(&mut cli);
            }
            reload_saved = false;
            stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let ending = Arc::clone(&shutdown);
            let folder = runtime_directory.clone();
            match std::thread::Builder::new()
                .name("bezel-light-render".into())
                .spawn(move || {
                    let Some(_screen) = handoff::light_screen(&folder)? else {
                        return Ok(String::new());
                    };
                    if handoff::studio_active(&folder)? {
                        return Ok(String::new());
                    }
                    crate::themes_with_shutdown(&cli, flag, ending)
                }) {
                Ok(thread) => worker = Some(thread),
                Err(error) => {
                    fatal = Some(anyhow::Error::from(error));
                    quit_requested.store(true, Ordering::SeqCst);
                }
            }
        }
    });
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    if let Some(error) = fatal {
        return Err(error);
    }
    Ok(String::new())
}
