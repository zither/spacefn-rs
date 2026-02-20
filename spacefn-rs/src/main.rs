mod config;
mod core;
mod ui;

use config::Config;
use core::{
    create_uinput_device, forward_event, list_input_devices, open_device, send_key, KeyValue,
    State, StateMachine,
};
use eframe::egui;
use eframe::egui::ViewportCommand;
use evdev::EventType;
use image::io::Reader as ImageReader;
use nix::sys::select::{select, FdSet};
use nix::sys::time::TimeVal;
use std::io::Cursor;
use std::os::fd::AsRawFd;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder,
};
use ui::{CoreCommand, SpacefnApp, UiMessage};

const KEY_SPACE: u16 = 57;
const DECIDE_TIMEOUT_MS: u64 = 200;

#[derive(Clone, Debug)]
enum TrayCommand {
    ShowWindow,
    Quit,
}

fn init_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    log::info!("spacefn-rs started");
}

fn check_device_permissions(device_path: &str) -> anyhow::Result<()> {
    match core::check_permissions(device_path) {
        Ok(()) => {
            log::info!("Device permissions OK");
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("Permission denied") {
                log::error!("Permission denied for device: {}", device_path);
                log::error!("Please add your user to 'input' and 'uinput' groups:");
            }
            Err(e)
        }
    }
}

fn wait_for_event(fd: std::os::unix::io::RawFd, timeout_ms: u64) -> bool {
    let mut readfds = FdSet::new();
    readfds.insert(fd);
    let mut timeout = TimeVal::new(0, (timeout_ms * 1000) as i64);
    match select(None, &mut readfds, None, None, Some(&mut timeout)) {
        Ok(n) => n > 0,
        Err(_) => false,
    }
}

fn run_state_machine(
    device_path: &str,
    config: Config,
    state_tx: mpsc::Sender<UiMessage>,
    cmd_rx: mpsc::Receiver<CoreCommand>,
) -> anyhow::Result<()> {
    let mut device = open_device(device_path)?;
    let mut uinput = create_uinput_device(&device)?;
    std::thread::sleep(Duration::from_millis(200));
    device.grab()?;
    let mut state = State::Idle;
    let mut buffer: Vec<u16> = Vec::new();
    let mut current_config = config;
    let _ = state_tx.send(UiMessage::StateChanged(state));

    loop {
        match state {
            State::Idle => {
                state = run_idle_state(
                    &mut device,
                    &mut uinput,
                    &current_config,
                    &state_tx,
                    &cmd_rx,
                )?
            }
            State::Decide => {
                state = run_decide_state(
                    &mut device,
                    &mut uinput,
                    &mut buffer,
                    &current_config,
                    &state_tx,
                    &cmd_rx,
                )?
            }
            State::Shift => {
                state = run_shift_state(
                    &mut device,
                    &mut uinput,
                    &mut buffer,
                    &current_config,
                    &state_tx,
                    &cmd_rx,
                )?
            }
        }
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                CoreCommand::ReloadConfig => {
                    if let Ok(new_config) = Config::load() {
                        current_config = new_config;
                    }
                }
                CoreCommand::Stop => return Ok(()),
            }
        }
    }
}

fn run_idle_state(
    device: &mut evdev::Device,
    uinput: &mut evdev::uinput::VirtualDevice,
    _config: &Config,
    state_tx: &mpsc::Sender<UiMessage>,
    _cmd_rx: &mpsc::Receiver<CoreCommand>,
) -> anyhow::Result<State> {
    loop {
        for event in device.fetch_events()? {
            if event.event_type() != EventType::KEY {
                forward_event(uinput, &event)?;
                continue;
            }
            let (code, value) = (event.code(), KeyValue::from(event.value()));
            let _ = state_tx.send(UiMessage::KeyPressed(code));
            if code == KEY_SPACE && value == KeyValue::Press {
                let _ = state_tx.send(UiMessage::StateChanged(State::Decide));
                return Ok(State::Decide);
            }
            send_key(uinput, code, event.value())?;
        }
    }
}

fn run_decide_state(
    device: &mut evdev::Device,
    uinput: &mut evdev::uinput::VirtualDevice,
    buffer: &mut Vec<u16>,
    config: &Config,
    state_tx: &mpsc::Sender<UiMessage>,
    _cmd_rx: &mpsc::Receiver<CoreCommand>,
) -> anyhow::Result<State> {
    buffer.clear();
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(DECIDE_TIMEOUT_MS);
    let fd = device.as_raw_fd();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            for &code in buffer.iter() {
                send_mapped_key(uinput, code, KeyValue::Press, config)?;
            }
            let _ = state_tx.send(UiMessage::StateChanged(State::Shift));
            return Ok(State::Shift);
        }
        let remaining = (timeout - elapsed).as_millis() as u64;
        if !wait_for_event(fd, remaining) {
            continue;
        }
        for event in device.fetch_events()? {
            if event.event_type() != EventType::KEY {
                forward_event(uinput, &event)?;
                continue;
            }
            let (code, value) = (event.code(), KeyValue::from(event.value()));
            let _ = state_tx.send(UiMessage::KeyPressed(code));
            if code == KEY_SPACE && value == KeyValue::Release {
                send_key(uinput, KEY_SPACE, 1)?;
                send_key(uinput, KEY_SPACE, 0)?;
                for &code in buffer.iter() {
                    send_key(uinput, code, 1)?;
                }
                let _ = state_tx.send(UiMessage::StateChanged(State::Idle));
                return Ok(State::Idle);
            }
            if value == KeyValue::Press {
                if !buffer.contains(&code) {
                    buffer.push(code);
                }
                continue;
            }
            if value == KeyValue::Release && !buffer.contains(&code) {
                send_key(uinput, code, event.value())?;
                continue;
            }
            if value == KeyValue::Release && buffer.contains(&code) {
                if let Some(pos) = buffer.iter().position(|&x| x == code) {
                    buffer.remove(pos);
                }
                send_mapped_key(uinput, code, KeyValue::Press, config)?;
                send_mapped_key(uinput, code, KeyValue::Release, config)?;
                let _ = state_tx.send(UiMessage::StateChanged(State::Shift));
                return Ok(State::Shift);
            }
        }
    }
}

fn run_shift_state(
    device: &mut evdev::Device,
    uinput: &mut evdev::uinput::VirtualDevice,
    buffer: &mut Vec<u16>,
    config: &Config,
    state_tx: &mpsc::Sender<UiMessage>,
    _cmd_rx: &mpsc::Receiver<CoreCommand>,
) -> anyhow::Result<State> {
    loop {
        for event in device.fetch_events()? {
            if event.event_type() != EventType::KEY {
                forward_event(uinput, &event)?;
                continue;
            }
            let (code, value) = (event.code(), KeyValue::from(event.value()));
            let _ = state_tx.send(UiMessage::KeyPressed(code));
            if code == KEY_SPACE && value == KeyValue::Release {
                for &code in buffer.iter() {
                    send_mapped_key(uinput, code, KeyValue::Release, config)?;
                }
                buffer.clear();
                let _ = state_tx.send(UiMessage::StateChanged(State::Idle));
                return Ok(State::Idle);
            }
            if code == KEY_SPACE {
                continue;
            }
            let mapped = send_mapped_key(uinput, code, value, config)?;
            if mapped {
                if value == KeyValue::Press {
                    if !buffer.contains(&code) {
                        buffer.push(code);
                    }
                } else if value == KeyValue::Release {
                    if let Some(pos) = buffer.iter().position(|&x| x == code) {
                        buffer.remove(pos);
                    }
                }
            }
        }
    }
}

fn send_mapped_key(
    uinput: &mut evdev::uinput::VirtualDevice,
    code: u16,
    value: KeyValue,
    config: &Config,
) -> anyhow::Result<bool> {
    let sm = StateMachine::new(config.clone());
    let (mapped_code, ext_code) = sm.map_key(code);
    let actual_code = if mapped_code != 0 { mapped_code } else { code };
    if let Some(ext) = ext_code {
        send_key(uinput, ext, value as i32)?;
    }
    send_key(uinput, actual_code, value as i32)?;
    Ok(mapped_code != 0 && mapped_code != code)
}

fn spawn_tray_thread(tray_tx: mpsc::Sender<TrayCommand>) {
    std::thread::spawn(move || {
        if gtk::init().is_err() {
            log::warn!("Failed to initialize GTK with default settings");
            std::env::set_var("GDK_BACKEND", "x11");
            if gtk::init().is_err() {
                log::error!("Failed to initialize GTK even with X11 backend");
                return;
            }
        }
        log::info!("GTK initialized successfully");

        let icon_bytes = include_bytes!("../resources/icon.png");
        let icon_image = ImageReader::new(Cursor::new(icon_bytes))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        let rgba = icon_image.to_rgba8();
        let (width, height) = rgba.dimensions();
        let icon = Icon::from_rgba(rgba.into_raw(), width, height).expect("Failed to create icon");

        let show_item = MenuItem::new("显示窗口", true, None);
        let quit_item = MenuItem::new("退出", true, None);
        let menu = Menu::with_items(&[&show_item, &quit_item]).expect("Failed to create menu");

        let show_item_id = show_item.id().clone();
        let quit_item_id = quit_item.id().clone();

        let _tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("SpaceFN")
            .with_menu(Box::new(menu))
            .build()
            .expect("Failed to build tray icon");
        log::info!("Tray icon created successfully");

        let tx = tray_tx.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            while gtk::events_pending() {
                gtk::main_iteration();
            }
            if let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == show_item_id {
                    log::info!("Show window clicked");
                    let _ = tx.send(TrayCommand::ShowWindow);
                } else if event.id == quit_item_id {
                    log::info!("Quit clicked");
                    let _ = tx.send(TrayCommand::Quit);
                }
            }
            gtk::glib::Continue(true)
        });

        gtk::main();
        std::mem::forget(_tray);
    });
}

fn run_ui(
    state_rx: mpsc::Receiver<UiMessage>,
    cmd_tx: mpsc::Sender<CoreCommand>,
    tray_rx: mpsc::Receiver<TrayCommand>,
) {
    let egui_ctx: Arc<std::sync::Mutex<Option<egui::Context>>> =
        Arc::new(std::sync::Mutex::new(None));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 600.0])
            .with_min_inner_size([500.0, 400.0]),
        ..Default::default()
    };

    let state_rx = std::sync::Mutex::new(state_rx);
    let cmd_tx = std::sync::Mutex::new(cmd_tx);
    let tray_rx = std::sync::Mutex::new(tray_rx);

    eframe::run_native(
        "SpaceFN",
        options,
        Box::new(move |cc| {
            *egui_ctx.lock().unwrap() = Some(cc.egui_ctx.clone());
            let mut app = SpacefnApp::new();
            app.reload_config();
            Box::new(SpacefnAppWrapper {
                app,
                state_rx,
                _cmd_tx: cmd_tx,
                tray_rx,
                egui_ctx,
                should_exit: false,
            })
        }),
    )
    .unwrap();
}

struct SpacefnAppWrapper {
    app: SpacefnApp,
    state_rx: std::sync::Mutex<mpsc::Receiver<UiMessage>>,
    _cmd_tx: std::sync::Mutex<mpsc::Sender<CoreCommand>>,
    tray_rx: std::sync::Mutex<mpsc::Receiver<TrayCommand>>,
    egui_ctx: Arc<std::sync::Mutex<Option<egui::Context>>>,
    should_exit: bool,
}

impl eframe::App for SpacefnAppWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.should_exit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        if let Ok(tray_rx) = self.tray_rx.lock() {
            while let Ok(cmd) = tray_rx.try_recv() {
                match cmd {
                    TrayCommand::ShowWindow => {
                        log::info!("Processing ShowWindow command");
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                    TrayCommand::Quit => {
                        log::info!("Processing Quit command");
                        self.should_exit = true;
                    }
                }
            }
        }

        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        if let Ok(state_rx) = self.state_rx.lock() {
            while let Ok(msg) = state_rx.try_recv() {
                match msg {
                    UiMessage::StateChanged(state) => self.app.update_state(state),
                    UiMessage::KeyPressed(key) => self.app.add_key_event(key),
                    UiMessage::Error(err) => self.app.set_error(err),
                }
            }
        }
        self.app.update(ctx, _frame);

        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() {
    init_logging();

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to load config: {}, using defaults", e);
            Config::default()
        }
    };

    let device_path = if !config.keyboard.is_empty() {
        config.keyboard.clone()
    } else {
        log::warn!("No keyboard device specified in config");
        let devices = list_input_devices();
        if devices.is_empty() {
            log::error!("No input devices found");
            return;
        }
        log::info!("Available devices:");
        for (i, dev) in devices.iter().enumerate() {
            log::info!("  {}: {} ({})", i, dev.name, dev.path);
        }
        return;
    };

    if let Err(e) = check_device_permissions(&device_path) {
        log::error!("Permission check failed: {}", e);
        return;
    }

    let (state_tx, state_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (tray_tx, tray_rx) = mpsc::channel();

    spawn_tray_thread(tray_tx);

    std::thread::sleep(Duration::from_millis(100));

    let device_path_clone = device_path.clone();
    let config_clone = config.clone();
    let core_handle = std::thread::spawn(move || {
        if let Err(e) = run_state_machine(&device_path_clone, config_clone, state_tx, cmd_rx) {
            log::error!("Core error: {}", e);
        }
    });

    run_ui(state_rx, cmd_tx, tray_rx);
    let _ = core_handle.join();
}
