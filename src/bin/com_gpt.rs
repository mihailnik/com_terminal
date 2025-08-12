#![windows_subsystem = "windows"]

use iced::alignment::Horizontal;
use iced::widget::{
    button, checkbox, column, container, pick_list, radio, row, scrollable, text, text_input,
};
use iced::{executor, Application, Element, Length, Settings, Subscription, Task};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_serial::SerialStream;

mod hex_utils {
    // simple hex helpers
    pub fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn hex_to_bytes(s: &str) -> Result<Vec<u8>, String> {
        let cleaned = s.split_whitespace().collect::<Vec<_>>().join("");
        if cleaned.len() % 2 != 0 {
            return Err("Hex string must have even length".into());
        }
        (0..cleaned.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| e.to_string()))
            .collect()
    }
}

mod file_ops {
    use rfd::FileDialog;
    use std::fs;

    pub fn open_file_blocking() -> Result<String, String> {
        if let Some(path) = FileDialog::new()
            .add_filter("Text", &["txt", "log"])
            .pick_file()
        {
            fs::read_to_string(path).map_err(|e| e.to_string())
        } else {
            Ok(String::new())
        }
    }

    pub fn save_file_blocking(default_name: &str, content: &str) -> Result<(), String> {
        if let Some(path) = FileDialog::new().set_file_name(default_name).save_file() {
            fs::write(path, content).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

mod serial_ops {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio_serial::{SerialPortBuilderExt, SerialStream};

    pub async fn open_port_async(
        port_name: String,
        baud: u32,
        parity: tokio_serial::Parity,
    ) -> Result<Arc<Mutex<SerialStream>>, String> {
        match tokio_serial::new(&port_name, baud)
            .parity(parity)
            .open_native_async()
        {
            Ok(s) => Ok(Arc::new(Mutex::new(s))),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn list_ports() -> Vec<String> {
        match serialport::available_ports() {
            Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    // UI & control
    RefreshPorts,
    Ports(Vec<String>),
    SelectPort(Option<String>),
    SelectBaud(u32),
    SelectParity(ParityOption),
    ToggleLineMode(bool),
    ToggleHexMode(bool),
    ConnectToggle,
    Disconnect,
    OpenFile,
    FileLoaded(String),
    InputChanged(String),
    ClearInput,
    SendInput,
    ClearTerminal,
    SaveTerminal,
    CopyTerminal,
    // Serial backend
    PortOpened(Result<Arc<Mutex<SerialStream>>, String>),
    SerialData(String),
    SerialError(String),
    // subscription tick (for periodic tasks)
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParityOption {
    None,
    Even,
    Odd,
}

impl ParityOption {
    fn all() -> Vec<ParityOption> {
        vec![ParityOption::None, ParityOption::Even, ParityOption::Odd]
    }
    fn to_tokio_parity(&self) -> tokio_serial::Parity {
        match self {
            ParityOption::None => tokio_serial::Parity::None,
            ParityOption::Even => tokio_serial::Parity::Even,
            ParityOption::Odd => tokio_serial::Parity::Odd,
        }
    }
}

impl std::fmt::Display for ParityOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParityOption::None => write!(f, "None"),
            ParityOption::Even => write!(f, "Even"),
            ParityOption::Odd => write!(f, "Odd"),
        }
    }
}

struct AppState {
    // UI state
    ports: Vec<String>,
    selected_port: Option<String>,
    baud_rates: Vec<u32>,
    selected_baud: u32,
    parity: ParityOption,
    line_mode: bool,
    hex_mode: bool,

    // serial
    port_handle: Option<Arc<Mutex<SerialStream>>>,

    // terminal
    terminal: String,
    input: String,

    // scanning
    scanning: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            selected_port: None,
            baud_rates: vec![9600, 19200, 38400, 57600, 115200, 128000, 256000],
            selected_baud: 115200,
            parity: ParityOption::None,
            line_mode: false,
            hex_mode: false,
            port_handle: None,
            terminal: String::new(),
            input: String::new(),
            scanning: true,
        }
    }
}

struct SerialApp {
    state: AppState,
}

impl Application for SerialApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Task<Message>) {
        let mut app = SerialApp {
            state: AppState::default(),
        };
        // initial ports refresh
        (app, Task::perform(async {}, |_| Message::RefreshPorts))
    }

    fn title(&self) -> String {
        "COM Terminal".into()
    }

    fn theme(&self) -> Self::Theme {
        iced::Theme::Dark
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        use Message::*;
        match message {
            RefreshPorts | Tick => {
                // spawn task to list ports
                return Task::perform(async { serial_ops::list_ports().await }, |ports| {
                    Message::Ports(ports)
                });
            }
            Ports(list) => {
                // update ports and keep selected if present
                self.state.ports = list;
                if self.state.selected_port.is_none() {
                    self.state.selected_port = self.state.ports.first().cloned();
                } else if let Some(selected) = &self.state.selected_port {
                    if !self.state.ports.contains(selected) {
                        self.state.selected_port = self.state.ports.first().cloned();
                    }
                }
                Task::none()
            }
            SelectPort(opt) => {
                self.state.selected_port = opt;
                Task::none()
            }
            SelectBaud(b) => {
                self.state.selected_baud = b;
                Task::none()
            }
            SelectParity(p) => {
                self.state.parity = p;
                Task::none()
            }
            ToggleLineMode(v) => {
                self.state.line_mode = v;
                Task::none()
            }
            ToggleHexMode(v) => {
                self.state.hex_mode = v;
                Task::none()
            }
            ConnectToggle => {
                // toggle: disconnect if connected, else open
                if self.state.port_handle.is_some() {
                    // disconnect
                    self.state.port_handle = None;
                    self.state.terminal.push_str("[Disconnected]\n");
                    return Task::perform(async {}, |_| Message::Disconnect);
                }
                // open if port selected
                if let Some(p) = self.state.selected_port.clone() {
                    let baud = self.state.selected_baud;
                    let parity = self.state.parity.to_tokio_parity();
                    self.state
                        .terminal
                        .push_str(&format!("[Opening {} @ {}]\n", p, baud));
                    return Task::perform(
                        serial_ops::open_port_async(p.clone(), baud, parity),
                        |res| match res {
                            Ok(handle) => Message::PortOpened(Ok(handle)),
                            Err(e) => Message::PortOpened(Err(e)),
                        },
                    );
                } else {
                    self.state.terminal.push_str("[No port selected]\n");
                }
                Task::none()
            }
            Disconnect => {
                self.state.port_handle = None;
                self.state.terminal.push_str("[Disconnected]\n");
                Task::none()
            }
            PortOpened(Ok(h)) => {
                self.state.port_handle = Some(h);
                self.state.terminal.push_str("[Connected]\n");
                Task::none()
            }
            PortOpened(Err(e)) => {
                self.state
                    .terminal
                    .push_str(&format!("[Open error: {}]\n", e));
                Task::none()
            }
            OpenFile => {
                return Task::perform(async { file_ops::open_file_blocking() }, |res| match res {
                    Ok(s) => Message::FileLoaded(s),
                    Err(e) => Message::SerialError(e),
                });
            }
            FileLoaded(text) => {
                self.state.input = text;
                Task::none()
            }
            InputChanged(s) => {
                self.state.input = s;
                Task::none()
            }
            ClearInput => {
                self.state.input.clear();
                Task::none()
            }
            SendInput => {
                let input_value = self.state.input.clone();
                // determine payload
                if let Some(port) = &self.state.port_handle {
                    let port = Arc::clone(port);
                    let hex_mode = self.state.hex_mode;
                    let line_mode = self.state.line_mode;
                    return Task::perform(
                        async move {
                            // build bytes
                            let bytes: Result<Vec<u8>, String> = if hex_mode {
                                hex_utils::hex_to_bytes(&input_value)
                            } else {
                                let mut v = input_value.into_bytes();
                                if line_mode && !v.ends_with(&[b'\n']) {
                                    v.push(b'\n');
                                }
                                Ok(v)
                            };
                            match bytes {
                                Ok(b) => {
                                    let mut guard = port.lock().await;
                                    if let Err(e) = guard.write_all(&b).await {
                                        Err(e.to_string())
                                    } else {
                                        Ok(hex_utils::bytes_to_hex(&b)) // return hex string for log
                                    }
                                }
                                Err(e) => Err(e),
                            }
                        },
                        |res: Result<String, String>| match res {
                            Ok(hexlog) => Message::SerialData(format!("=> {}\n", hexlog)),
                            Err(e) => Message::SerialError(format!("Send error: {}", e)),
                        },
                    );
                } else {
                    self.state.terminal.push_str("[Not connected]\n");
                }
                Task::none()
            }
            SerialData(s) => {
                // when serial data arrives we append either raw or hex-view depending on hex_mode
                if self.state.hex_mode {
                    // assume s is raw text of bytes; but our reader will send raw bytes converted to a string:
                    // better to treat the incoming string as raw bytes converted lossily, so we convert to hex via bytes
                    let bytes = s.into_bytes();
                    let hex = hex_utils::bytes_to_hex(&bytes);
                    self.state.terminal.push_str(&format!("<= {}\n", hex));
                } else {
                    if self.state.line_mode {
                        // push as-is; incoming may already contain newlines
                        self.state.terminal.push_str(&s);
                    } else {
                        self.state.terminal.push_str(&s);
                    }
                }
                Task::none()
            }
            SerialError(e) => {
                self.state
                    .terminal
                    .push_str(&format!("[Serial error: {}]\n", e));
                self.state.port_handle = None;
                Task::none()
            }
            ClearTerminal => {
                self.state.terminal.clear();
                Task::none()
            }
            SaveTerminal => {
                let data = self.state.terminal.clone();
                return Task::perform(
                    async move { file_ops::save_file_blocking("terminal_log.txt", &data) },
                    |_| Message::Tick,
                );
            }
            CopyTerminal => {
                let clip = self.state.terminal.clone();
                return Task::perform(
                    async move {
                        // blocking clipboard set
                        let mut ctx: clipboard::ClipboardContext =
                            clipboard::ClipboardProvider::new().map_err(|e| e.to_string())?;
                        ctx.set_contents(clip).map_err(|e| e.to_string())
                    },
                    |_| Message::Tick,
                );
            }
            Tick => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        // if connected -> subscribe to read-loop
        let serial_sub = if let Some(port) = &self.state.port_handle {
            read_serial_subscription(Arc::clone(port))
        } else {
            Subscription::none()
        };

        // periodic port scanning
        let scan =
            iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::RefreshPorts);

        Subscription::batch(vec![serial_sub, scan])
    }

    fn view(&self) -> Element<Message> {
        // top row: ports, baud, parity, connect
        let port_pick = pick_list(
            self.state.ports.clone(),
            self.state.selected_port.clone(),
            Message::SelectPort,
        )
        .placeholder("Select COM");

        let baud_pick = pick_list(
            self.state.baud_rates.clone(),
            Some(self.state.selected_baud),
            Message::SelectBaud,
        );

        let parity_row = row![
            text("Parity:"),
            radio(
                "None",
                ParityOption::None,
                Some(self.state.parity),
                Message::SelectParity
            ),
            radio(
                "Even",
                ParityOption::Even,
                Some(self.state.parity),
                Message::SelectParity
            ),
            radio(
                "Odd",
                ParityOption::Odd,
                Some(self.state.parity),
                Message::SelectParity
            )
        ]
        .spacing(10)
        .align_items(iced::Alignment::Center);

        let connect_btn = if self.state.port_handle.is_some() {
            button("Disconnect").on_press(Message::ConnectToggle)
        } else {
            button("Connect").on_press(Message::ConnectToggle)
        };

        let top = row![port_pick, baud_pick, parity_row, connect_btn].spacing(15);

        // terminal (scrollable)
        let terminal = scrollable(text(&self.state.terminal))
            .width(Length::Fill)
            .height(Length::FillPortion(6));

        // terminal controls
        let terminal_controls = row![
            button("Clear Terminal").on_press(Message::ClearTerminal),
            button("Save...").on_press(Message::SaveTerminal),
            button("Copy").on_press(Message::CopyTerminal),
            checkbox("Line mode", self.state.line_mode, Message::ToggleLineMode),
            checkbox("Hex mode", self.state.hex_mode, Message::ToggleHexMode),
        ]
        .spacing(10);

        // input row (Open file, input, send, clear input)
        let open_file_btn = button("Open File").on_press(Message::OpenFile);
        let input_field = text_input(
            "Type or load data...",
            &self.state.input,
            Message::InputChanged,
        )
        .on_submit(Message::SendInput)
        .width(Length::FillPortion(4));
        let send_btn = button("Send").on_press(Message::SendInput);
        let clear_input_btn = button("Clear Input").on_press(Message::ClearInput);

        let input_row = row![open_file_btn, input_field, send_btn, clear_input_btn].spacing(10);

        let content = column![top, terminal, terminal_controls, input_row]
            .spacing(12)
            .padding(12);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

// Subscription: read serial loop and forward bytes to UI as Message::SerialData(String)
fn read_serial_subscription(port: Arc<Mutex<SerialStream>>) -> Subscription<Message> {
    iced::subscription::channel(100, move |mut output| async move {
        let mut buf = [0u8; 1024];
        loop {
            // lock and read
            let mut guard = port.lock().await;
            match guard.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let data = buf[..n].to_vec();
                    drop(guard);
                    // convert to lossily decoded String to pass via Message
                    let s = String::from_utf8_lossy(&data).to_string();
                    if output.send(Message::SerialData(s)).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {
                    drop(guard);
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => {
                    drop(guard);
                    let _ = output.send(Message::SerialError(e.to_string())).await;
                    break;
                }
            }
        }
    })
}

fn main() -> iced::Result {
    SerialApp::run(Settings {
        window: iced::window::Settings {
            size: (1000, 700),
            ..Default::default()
        },
        ..Settings::default()
    })
}
