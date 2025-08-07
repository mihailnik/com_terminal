// main.rs от гпт
use iced::executor;
use iced::widget::scrollable::Id;
use iced::widget::{
    button, column, container, pick_list, progress_bar, radio, row, scrollable, text, text_input,
};
use iced::{Alignment, Application, Command, Element, Length, Theme};
use serialport::{Parity, SerialPort, StopBits, available_ports};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
enum BaudRate {
    B9600,
    B19200,
    B38400,
    B57600,
    B115200,
}

impl BaudRate {
    fn all() -> Vec<BaudRate> {
        vec![
            BaudRate::B9600,
            BaudRate::B19200,
            BaudRate::B38400,
            BaudRate::B57600,
            BaudRate::B115200,
        ]
    }

    fn as_u32(&self) -> u32 {
        match self {
            BaudRate::B9600 => 9600,
            BaudRate::B19200 => 19200,
            BaudRate::B38400 => 38400,
            BaudRate::B57600 => 57600,
            BaudRate::B115200 => 115200,
        }
    }
}

impl std::fmt::Display for BaudRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_u32())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParityOption {
    None,
    Odd,
    Even,
}

impl ParityOption {
    fn all() -> [ParityOption; 3] {
        [ParityOption::None, ParityOption::Odd, ParityOption::Even]
    }

    fn to_parity(self) -> Parity {
        match self {
            ParityOption::None => Parity::None,
            ParityOption::Odd => Parity::Odd,
            ParityOption::Even => Parity::Even,
        }
    }
}

impl std::fmt::Display for ParityOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopBitsOption {
    One,
    Two,
}

impl StopBitsOption {
    fn all() -> [StopBitsOption; 2] {
        [StopBitsOption::One, StopBitsOption::Two]
    }

    fn to_stop_bits(self) -> StopBits {
        match self {
            StopBitsOption::One => StopBits::One,
            StopBitsOption::Two => StopBits::Two,
        }
    }
}

impl std::fmt::Display for StopBitsOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
enum Message {
    PortNameChanged(String),
    BaudRateChanged(BaudRate),
    ParityChanged(ParityOption),
    StopBitsChanged(StopBitsOption),
    OpenPort,
    FileSelected(Option<std::path::PathBuf>),
    SendFile,
    Log(String),
    ManualInputChanged(String),
    SendManual,
    SaveLog,
    ClearLog,
    Receive(String),
    SelectFileDialog,
    ProgressChanged(f32),
}

struct ComApp {
    port_name: String,
    available_ports: Vec<String>,
    baud_rate: BaudRate,
    parity: ParityOption,
    stop_bits: StopBitsOption,
    port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
    file_path: Option<std::path::PathBuf>,
    logs: Vec<String>,
    manual_input: String,
    scroll: Id,
    progress: f32,
}

impl Application for ComApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let ports: Vec<String> = available_ports()
            .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default();

        (
            Self {
                port_name: ports.get(0).cloned().unwrap_or_default(),
                available_ports: ports,
                baud_rate: BaudRate::B115200,
                parity: ParityOption::None,
                stop_bits: StopBitsOption::One,
                port: None,
                file_path: None,
                logs: vec![],
                manual_input: String::new(),
                scroll: Id::unique(),
                progress: 0.0,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "COM порт Терминал".into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PortNameChanged(name) => self.port_name = name,
            Message::BaudRateChanged(rate) => self.baud_rate = rate,
            Message::ParityChanged(p) => self.parity = p,
            Message::StopBitsChanged(s) => self.stop_bits = s,
            Message::OpenPort => {
                if self.port.is_none() {
                    match self.open_serial_port() {
                        Ok(p) => {
                            self.logs.push(format!("Порт {} открыт", self.port_name));
                            let arc_port = Arc::new(Mutex::new(p));
                            let read_port = arc_port.clone();
                            let sender = iced::futures::channel::mpsc::unbounded();
                            let mut tx = sender.0.clone();

                            thread::spawn(move || {
                                let mut buf = [0u8; 1024];
                                loop {
                                    let mut port = read_port.lock().unwrap();
                                    match port.read(&mut buf) {
                                        Ok(n) if n > 0 => {
                                            let text =
                                                String::from_utf8_lossy(&buf[..n]).to_string();
                                            let _ = tx.start_send(Message::Receive(text));
                                        }
                                        _ => {
                                            drop(port);
                                            thread::sleep(Duration::from_millis(100));
                                        }
                                    }
                                }
                            });
                            self.port = Some(arc_port);
                        }
                        Err(e) => self.logs.push(format!("Ошибка открытия: {}", e)),
                    }
                } else {
                    self.logs.push(format!("Порт {} закрыт", self.port_name));
                    self.port = None;
                }
            }
            Message::FileSelected(path) => self.file_path = path,
            Message::SelectFileDialog => {}
            Message::SendFile => {}
            Message::SendManual => {
                if let Some(ref port) = self.port {
                    let mut port = port.lock().unwrap();
                    let _ = port.write_all(self.manual_input.as_bytes());
                    self.logs.push(format!("> {}", self.manual_input));
                    self.manual_input.clear();
                } else {
                    self.logs.push("Порт не открыт".to_string());
                }
            }
            Message::ManualInputChanged(value) => self.manual_input = value,
            Message::SaveLog => {}
            Message::ClearLog => self.logs.clear(),
            Message::Log(msg) => self.logs.push(msg),
            Message::Receive(data) => self.logs.push(format!("< {}", data)),
            Message::ProgressChanged(val) => self.progress = val,
        }

        Command::none()
    }

    fn view(&self) -> Element<Message> {
        // Оставим как есть - визуализация не требует изменений
        // ...
        unimplemented!()
    }
}

impl ComApp {
    fn open_serial_port(&self) -> Result<Box<dyn SerialPort>, serialport::Error> {
        serialport::new(&self.port_name, self.baud_rate.as_u32())
            .timeout(Duration::from_millis(1000))
            .parity(self.parity.to_parity())
            .stop_bits(self.stop_bits.to_stop_bits())
            .open()
    }
}

fn main() -> iced::Result {
    ComApp::run(iced::Settings::default())
}
