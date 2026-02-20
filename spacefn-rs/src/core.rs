use evdev::{AttributeSet, Device, EventType, InputEvent, Key};
use std::fs::File;

const MAX_BUFFER: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyValue {
    Release = 0,
    Press = 1,
    Repeat = 2,
}

impl From<i32> for KeyValue {
    fn from(v: i32) -> Self {
        match v {
            0 => KeyValue::Release,
            1 => KeyValue::Press,
            2 => KeyValue::Repeat,
            _ => KeyValue::Release,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Decide,
    Shift,
}

pub struct KeyBuffer {
    buffer: Vec<u16>,
}

impl KeyBuffer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn contains(&self, code: u16) -> bool {
        self.buffer.contains(&code)
    }

    pub fn append(&mut self, code: u16) -> bool {
        if self.buffer.len() >= MAX_BUFFER {
            return false;
        }
        if self.buffer.contains(&code) {
            return false;
        }
        self.buffer.push(code);
        true
    }

    pub fn remove(&mut self, code: u16) -> bool {
        if let Some(pos) = self.buffer.iter().position(|&x| x == code) {
            self.buffer.remove(pos);
            return true;
        }
        false
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &u16> {
        self.buffer.iter()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for KeyBuffer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct StateMachine {
    state: State,
    buffer: KeyBuffer,
    pub config: crate::config::Config,
}

impl StateMachine {
    pub fn new(config: crate::config::Config) -> Self {
        Self {
            state: State::Idle,
            buffer: KeyBuffer::new(),
            config,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn map_key(&self, original: u16) -> (u16, Option<u16>) {
        for mapping in &self.config.keys_map {
            if mapping[0] == u32::from(original) {
                let mapped = if mapping[1] != 0 {
                    mapping[1] as u16
                } else {
                    original
                };
                let extended = if mapping[2] != 0 {
                    Some(mapping[2] as u16)
                } else {
                    None
                };
                return (mapped, extended);
            }
        }
        (original, None)
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        if state == State::Decide {
            self.buffer.clear();
        }
    }

    pub fn buffer(&self) -> &KeyBuffer {
        &self.buffer
    }
}

pub fn check_permissions(device_path: &str) -> anyhow::Result<()> {
    let _device_file = File::open(device_path)?;

    let _uinput_file = File::open("/dev/uinput")?;

    Ok(())
}

pub fn list_input_devices() -> Vec<InputDeviceInfo> {
    let mut devices = Vec::new();

    let enumeration = evdev::enumerate();
    for (path, device) in enumeration {
        if let Some(name) = device.name() {
            devices.push(InputDeviceInfo {
                path: path.to_string_lossy().to_string(),
                name: name.to_string(),
            });
        }
    }

    devices
}

#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    pub path: String,
    pub name: String,
}

pub fn open_device(path: &str) -> anyhow::Result<Device> {
    let device = Device::open(path)?;
    Ok(device)
}

pub fn create_uinput_device(input_device: &Device) -> anyhow::Result<evdev::uinput::VirtualDevice> {
    let keys = input_device.supported_keys();

    let mut key_set = AttributeSet::<Key>::new();
    if let Some(k) = keys {
        for key in k.iter() {
            key_set.insert(key);
        }
    }

    let device = evdev::uinput::VirtualDeviceBuilder::new()?
        .name("spacefn virtual keyboard")
        .with_keys(&key_set)?
        .build()?;

    Ok(device)
}

pub fn send_key(
    uinput: &mut evdev::uinput::VirtualDevice,
    code: u16,
    value: i32,
) -> anyhow::Result<()> {
    let event = InputEvent::new(EventType::KEY, code, value);
    uinput.emit(&[event])?;
    Ok(())
}

pub fn forward_event(
    uinput: &mut evdev::uinput::VirtualDevice,
    event: &InputEvent,
) -> anyhow::Result<()> {
    uinput.emit(&[event.clone()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_buffer() {
        let mut buffer = KeyBuffer::new();

        assert!(!buffer.contains(1));
        assert!(buffer.append(1));
        assert!(buffer.contains(1));
        assert!(!buffer.append(1));

        assert!(buffer.remove(1));
        assert!(!buffer.contains(1));

        for i in 0..MAX_BUFFER {
            assert!(buffer.append(i as u16));
        }
        assert!(!buffer.append(100));
    }

    #[test]
    fn test_key_buffer_clear() {
        let mut buffer = KeyBuffer::new();
        buffer.append(1);
        buffer.append(2);
        assert_eq!(buffer.len(), 2);

        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_key_buffer_remove_middle() {
        let mut buffer = KeyBuffer::new();
        buffer.append(1);
        buffer.append(2);
        buffer.append(3);

        buffer.remove(2);
        assert!(!buffer.contains(2));
        assert!(buffer.contains(1));
        assert!(buffer.contains(3));
    }

    #[test]
    fn test_state_transitions() {
        let config = crate::config::Config::default();
        let mut sm = StateMachine::new(config);

        assert_eq!(sm.state(), State::Idle);

        sm.set_state(State::Decide);
        assert_eq!(sm.state(), State::Decide);
        assert!(sm.buffer.is_empty());

        sm.set_state(State::Shift);
        assert_eq!(sm.state(), State::Shift);
    }

    #[test]
    fn test_key_map_no_mapping() {
        let config = crate::config::Config::default();
        let sm = StateMachine::new(config);

        let (mapped, ext) = sm.map_key(30); // Key A
        assert_eq!(mapped, 30);
        assert_eq!(ext, None);
    }

    #[test]
    fn test_key_map_with_mapping() {
        let config = crate::config::Config {
            keyboard: String::new(),
            keys_map: vec![[30, 105, 0]], // A -> F9
        };
        let sm = StateMachine::new(config);

        let (mapped, ext) = sm.map_key(30);
        assert_eq!(mapped, 105);
        assert_eq!(ext, None);
    }

    #[test]
    fn test_key_map_with_extended() {
        let config = crate::config::Config {
            keyboard: String::new(),
            keys_map: vec![[104, 0, 109]], // PageUp -> Pause
        };
        let sm = StateMachine::new(config);

        let (mapped, ext) = sm.map_key(104);
        assert_eq!(mapped, 104); // 0 means no remap, keep original
        assert_eq!(ext, Some(109));
    }

    #[test]
    fn test_key_map_both_mapped_and_extended() {
        let config = crate::config::Config {
            keyboard: String::new(),
            keys_map: vec![[57, 0, 125]], // Space -> Fn+Space = Menu
        };
        let sm = StateMachine::new(config);

        let (mapped, ext) = sm.map_key(57);
        assert_eq!(mapped, 57); // Keep original key
        assert_eq!(ext, Some(125)); // Send extended key
    }

    #[test]
    fn test_config_default() {
        let config = crate::config::Config::default();
        assert!(config.keyboard.is_empty());
        assert!(config.keys_map.is_empty());
    }
}
