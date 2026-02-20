use crate::core::State;
#[cfg(feature = "ui")]
use eframe::egui;

#[cfg(feature = "ui")]
pub use crate::{CoreCommand, UiMessage};

#[cfg(feature = "ui")]
pub struct SpacefnApp {
    pub current_state: State,
    pub key_history: Vec<KeyEvent>,
    pub devices: Vec<crate::core::InputDeviceInfo>,
    pub selected_device: Option<usize>,
    pub config: crate::config::Config,
    pub show_config: bool,
    pub error_message: Option<String>,
    pub new_key: (u32, u32, u32),
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub code: u16,
    pub value: KeyValue,
    pub timestamp: std::time::Instant,
}

#[derive(Clone, Debug, Copy)]
pub enum KeyValue {
    Release,
    Press,
    Repeat,
}

impl KeyEvent {
    pub fn new(code: u16, value: i32) -> Self {
        Self {
            code,
            value: match value {
                0 => KeyValue::Release,
                1 => KeyValue::Press,
                2 => KeyValue::Repeat,
                _ => KeyValue::Press,
            },
            timestamp: std::time::Instant::now(),
        }
    }

    pub fn display_string(&self) -> String {
        let value_str = match self.value {
            KeyValue::Press => "↓",
            KeyValue::Release => "↑",
            KeyValue::Repeat => "↻",
        };
        format!("{} {:03} {}", value_str, self.code, get_key_name(self.code))
    }
}

pub fn get_key_name(code: u16) -> &'static str {
    match code {
        0 => "Reserved",
        1 => "Esc",
        2 => "1",
        3 => "2",
        4 => "3",
        5 => "4",
        6 => "5",
        7 => "6",
        8 => "7",
        9 => "8",
        10 => "9",
        11 => "0",
        12 => "-",
        13 => "=",
        14 => "Back",
        15 => "Tab",
        16 => "Q",
        17 => "W",
        18 => "E",
        19 => "R",
        20 => "T",
        21 => "Y",
        22 => "U",
        23 => "I",
        24 => "O",
        25 => "P",
        26 => "[",
        27 => "]",
        28 => "Enter",
        29 => "LCtrl",
        30 => "A",
        31 => "S",
        32 => "D",
        33 => "F",
        34 => "G",
        35 => "H",
        36 => "J",
        37 => "K",
        38 => "L",
        39 => ";",
        40 => "'",
        41 => "`",
        42 => "LShift",
        43 => "\\",
        44 => "Z",
        45 => "X",
        46 => "C",
        47 => "V",
        48 => "B",
        49 => "N",
        50 => "M",
        51 => ",",
        52 => ".",
        53 => "/",
        54 => "RShift",
        55 => "KP*",
        56 => "LAlt",
        57 => "Space",
        58 => "Caps",
        59 => "F1",
        60 => "F2",
        61 => "F3",
        62 => "F4",
        63 => "F5",
        64 => "F6",
        65 => "F7",
        66 => "F8",
        67 => "F9",
        68 => "F10",
        69 => "NumLock",
        70 => "ScrLock",
        71 => "KP7",
        72 => "KP8",
        73 => "KP9",
        74 => "KP-",
        75 => "KP4",
        76 => "KP5",
        77 => "KP6",
        78 => "KP+",
        79 => "KP1",
        80 => "KP2",
        81 => "KP3",
        82 => "KP0",
        83 => "KP.",
        84 => "OEM102",
        85 => "F11",
        86 => "F12",
        87 => "F11",
        88 => "F12",
        89 => "Kata",
        90 => "Hira",
        91 => "Henkan",
        92 => "Kana",
        93 => "Muhen",
        94 => "KPEnt",
        95 => "RCtrl",
        96 => "KP/",
        97 => "SysRq",
        98 => "RAlt",
        99 => "LFn",
        100 => "Home",
        101 => "Up",
        102 => "PgUp",
        103 => "Up",
        104 => "Right",
        105 => "End",
        106 => "Down",
        107 => "PgDn",
        108 => "Ins",
        109 => "Del",
        110 => "Macro",
        111 => "Mute",
        112 => "Vol-",
        113 => "Vol+",
        114 => "Power",
        115 => "KP=",
        116 => "KP+/-",
        117 => "Pause",
        118 => "Scale",
        119 => "KP,",
        120 => "RO",
        125 => "Menu",
        _ => "?",
    }
}

impl SpacefnApp {
    pub fn new() -> Self {
        Self {
            current_state: State::Idle,
            key_history: Vec::new(),
            devices: crate::core::list_input_devices(),
            selected_device: None,
            config: crate::config::Config::default(),
            show_config: false,
            error_message: None,
            new_key: (0, 0, 0),
        }
    }

    pub fn update_state(&mut self, state: State) {
        self.current_state = state;
    }

    pub fn add_key_event(&mut self, code: u16) {
        self.add_key_event_full(code, 1);
    }

    pub fn add_key_event_full(&mut self, code: u16, value: i32) {
        let event = KeyEvent::new(code, value);
        self.key_history.insert(0, event);
        if self.key_history.len() > 20 {
            self.key_history.pop();
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn reload_config(&mut self) {
        match crate::config::Config::load() {
            Ok(config) => {
                self.config = config;
                self.clear_error();
            }
            Err(e) => {
                self.set_error(format!("Failed to reload config: {}", e));
            }
        }
    }

    fn state_color(&self) -> egui::Color32 {
        match self.current_state {
            State::Idle => egui::Color32::from_rgb(76, 175, 80),
            State::Decide => egui::Color32::from_rgb(255, 193, 7),
            State::Shift => egui::Color32::from_rgb(244, 67, 54),
        }
    }

    fn state_text(&self) -> &'static str {
        match self.current_state {
            State::Idle => "IDLE",
            State::Decide => "DECIDE",
            State::Shift => "FN MODE",
        }
    }
}

impl Default for SpacefnApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for SpacefnApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("SpaceFN");
                ui.separator();

                ui.colored_label(self.state_color(), self.state_text());

                ui.separator();

                if ui.button("Status").clicked() {
                    self.show_config = false;
                }
                if ui.button("Config").clicked() {
                    self.show_config = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Quit").clicked() {
                        std::process::exit(0);
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_config {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.show_config_ui(ui);
                });
            } else {
                self.show_status_ui(ui);
            }
        });
    }
}

impl SpacefnApp {
    fn show_status_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Current Status");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Status: ");
            ui.colored_label(self.state_color(), self.state_text());
        });

        ui.label(format!(
            "Device: {}",
            if self.config.keyboard.is_empty() {
                "Not selected"
            } else {
                &self.config.keyboard
            }
        ));
        ui.label(format!("Mappings: {} keys", self.config.keys_map.len()));

        ui.separator();
        ui.label("Recent Keys");
        ui.separator();

        for event in &self.key_history {
            ui.label(event.display_string());
        }

        if self.key_history.is_empty() {
            ui.colored_label(egui::Color32::GRAY, "No key events");
        }

        if let Some(ref err) = self.error_message {
            ui.separator();
            ui.colored_label(egui::Color32::RED, err);
        }
    }

    fn show_config_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Keyboard Device");
        ui.separator();

        egui::ComboBox::from_label("Select device")
            .selected_text(format!(
                "{}",
                self.selected_device
                    .as_ref()
                    .map(|i| self.devices[*i].name.clone())
                    .unwrap_or_else(|| "Choose...".to_string())
            ))
            .show_ui(ui, |ui| {
                for (i, device) in self.devices.iter().enumerate() {
                    ui.selectable_value(&mut self.selected_device, Some(i), &device.name);
                }
            });

        if let Some(idx) = self.selected_device {
            if ui.button("Use This Device").clicked() {
                self.config.keyboard = self.devices[idx].path.clone();
            }
        }

        ui.separator();
        ui.label("Key Mappings");
        ui.label("Space+Original -> Mapped [Extended]");

        let mut to_remove: Vec<usize> = Vec::new();

        for (i, mapping) in self.config.keys_map.iter().enumerate() {
            ui.horizontal(|ui| {
                let orig = get_key_name(mapping[0] as u16);
                let mapped = if mapping[1] == 0 {
                    "orig".to_string()
                } else {
                    get_key_name(mapping[1] as u16).to_string()
                };
                let ext = if mapping[2] == 0 {
                    "-".to_string()
                } else {
                    get_key_name(mapping[2] as u16).to_string()
                };

                ui.label(format!("{} -> {} [{}]", orig, mapped, ext));

                if ui.button("X").clicked() {
                    to_remove.push(i);
                }
            });
        }

        for i in to_remove.iter().rev() {
            self.config.keys_map.remove(*i);
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Add:");
            ui.add(
                egui::DragValue::new(&mut self.new_key.0)
                    .clamp_range(0..=255)
                    .speed(1.0),
            );
            ui.add(
                egui::DragValue::new(&mut self.new_key.1)
                    .clamp_range(0..=255)
                    .speed(1.0),
            );
            ui.add(
                egui::DragValue::new(&mut self.new_key.2)
                    .clamp_range(0..=255)
                    .speed(1.0),
            );
            if ui.button("Add").clicked() {
                self.config
                    .keys_map
                    .push([self.new_key.0, self.new_key.1, self.new_key.2]);
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                if let Some(home) = dirs::home_dir() {
                    let path = home.join(".config/spacefn/config.toml");
                    match self.config.save(&path) {
                        Ok(_) => self.clear_error(),
                        Err(e) => self.set_error(e.to_string()),
                    }
                }
            }
            if ui.button("Reload").clicked() {
                self.reload_config();
            }
            if ui.button("Refresh").clicked() {
                self.devices = crate::core::list_input_devices();
            }
        });
    }
}
