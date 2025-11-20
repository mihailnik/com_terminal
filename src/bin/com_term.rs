#![windows_subsystem = "windows"]
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::Stream; // ← саме цей Stream
use iced::advanced::subscription::{self, Recipe};
use iced::futures::stream::{self, BoxStream};
use iced::futures::{self, StreamExt};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Theme};
// use rustc_hash::FxHasher;
// use iced::Font;
use serialport::{available_ports, SerialPort};
use std::collections::VecDeque;
use std::hash::Hash;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Message {
    // Навигация
    ShowTerminal,
    ShowSettings,
    ShowMonitor,
    ShowFileView,

    // Terminal
    InputChanged(String),
    SendData,
    ClearTerminal,

    // Settings
    PortSelected(String),
    BaudRateSelected(u32),
    ConnectPort,
    DisconnectPort,
    RefreshPorts,
    PortsUpdated(Vec<String>),

    // Monitor
    StartMonitoring,
    StopMonitoring,

    // File
    OpenFile,
    SaveLog,

    // Serial port
    DataReceived(String),
    PortError(String),
}

#[derive(Debug, Clone, Default)]
pub enum WindowState {
    #[default]
    Terminal,
    Settings,
    Monitor,
    FileView,
}

#[derive(Debug, Clone)]
pub struct PortSettings {
    pub port_name: Option<String>,
    pub baud_rate: u32,
    pub connected: bool,
}

impl Default for PortSettings {
    fn default() -> Self {
        Self {
            port_name: None,
            baud_rate: 115200,
            connected: false,
        }
    }
}
struct ComReceiver {
    rx: Arc<Receiver<Message>>,
}

impl Recipe for ComReceiver {
    type Output = Message;

    fn hash(&self, state: &mut subscription::Hasher) {
        "com_receiver".hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: Pin<Box<dyn futures::Stream<Item = iced::advanced::subscription::Event> + Send>>,
    ) -> BoxStream<'static, Self::Output> {
        stream::unfold(self.rx.clone(), |rx| async move {
            match rx.recv() {
                Ok(msg) => Some((msg, rx)),
                Err(_) => None,
            }
        })
        .boxed()
    }
}

#[derive(Default)]
pub struct ComTerminal {
    current_window: WindowState,
    input_text: String,
    terminal_output: VecDeque<String>,
    port_settings: PortSettings,
    available_ports: Vec<String>,
    baud_rates: Vec<u32>,
    monitoring: bool,
    received_bytes: u64,
    sent_bytes: u64,
    log_file_path: Option<String>,
    serial_port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
    rx: Option<Receiver<Message>>, // нове поле для прийому повідомлень
    tx: Option<Sender<Message>>,
    write_tx: Option<Sender<String>>, // канал для команд записи в порт
}

impl ComTerminal {
    fn new() -> Self {
        let mut terminal = Self {
            current_window: WindowState::Terminal,
            input_text: String::new(),
            terminal_output: VecDeque::new(),
            port_settings: PortSettings::default(),
            available_ports: vec![],
            baud_rates: vec![9600, 19200, 38400, 57600, 115200],
            monitoring: false,
            received_bytes: 0,
            sent_bytes: 0,
            log_file_path: None,
            serial_port: None,
            rx: None,
            tx: None,
            write_tx: None,
        };

        terminal
            .terminal_output
            .push_back("=== COM Terminal запущено ===".to_string());
        terminal
            .terminal_output
            .push_back("Завантаження списку COM портів...".to_string());

        match available_ports() {
            Ok(ports) => {
                terminal.available_ports = ports.into_iter().map(|p| p.port_name).collect();
                if terminal.available_ports.is_empty() {
                    terminal
                        .terminal_output
                        .push_back("! COM порти не Знайдено".to_string());
                } else {
                    terminal.terminal_output.push_back(format!(
                        "📋 Знайдено портів: {}",
                        terminal.available_ports.len()
                    ));
                }
            }
            Err(e) => {
                terminal
                    .terminal_output
                    .push_back(format!("✗ Помилка отримання списку портів: {}", e));
            }
        }

        terminal
    }

    fn title(&self) -> String {
        String::from("COM Terminal")
    }

    // // 🔧 ось тут вставляєш
    // fn font(&self) -> Font {
    //     EMOJI_FONT
    // }
    fn update(&mut self, message: Message) {
        match message {
            Message::ShowTerminal => {
                self.current_window = WindowState::Terminal;
            }
            Message::ShowSettings => {
                self.current_window = WindowState::Settings;
            }
            Message::ShowMonitor => {
                self.current_window = WindowState::Monitor;
            }
            Message::ShowFileView => {
                self.current_window = WindowState::FileView;
            }
            Message::InputChanged(text) => {
                self.input_text = text;
            }
            Message::SendData => {
                if !self.input_text.is_empty() && self.port_settings.connected {
                    let data = self.input_text.clone();
                    self.terminal_output.push_back(format!(">>> {}", data));
                    self.sent_bytes += data.len() as u64;

                    if let Some(port) = &self.serial_port {
                        let mut port_lock = port.lock().unwrap();
                        match port_lock.write_all(data.as_bytes()) {
                            Ok(_) => {
                                self.terminal_output
                                    .push_back(format!("✓ Данні відправлені"));
                            }
                            Err(e) => {
                                self.terminal_output
                                    .push_back(format!("✗ Помилка відправлення данних: {}", e));
                            }
                        }
                    }
                    self.input_text.clear();
                }
            }
            Message::ClearTerminal => {
                self.terminal_output.clear();
                self.terminal_output
                    .push_back("=== Термінал очищено ===".to_string());
            }
            Message::PortSelected(port) => {
                self.port_settings.port_name = Some(port);
            }
            Message::BaudRateSelected(rate) => {
                self.port_settings.baud_rate = rate;
            }
            Message::ConnectPort => {
                if let Some(port_name) = &self.port_settings.port_name.clone() {
                    match serialport::new(port_name, self.port_settings.baud_rate)
                        .timeout(Duration::from_millis(10))
                        .open()
                    {
                        Ok(mut port) => {
                            self.port_settings.connected = true;

                            // создаём канал сообщений от потока -> UI
                            let (tx, rx) = unbounded();
                            self.tx = Some(tx.clone());
                            self.rx = Some(rx);

                            // создаём канал команд записи: UI -> поток порта
                            let (write_tx, write_rx) = unbounded::<String>();
                            self.write_tx = Some(write_tx.clone());

                            // перезаписываем порт-owned поток: поток владеет `port` (не через Arc/Mutex)
                            // перемещаем port в поток
                            let port_name_clone = port_name.clone();
                            let tx_clone = tx.clone();
                            thread::spawn(move || {
                                let mut buf = [0u8; 1024];

                                // оповестим UI, что поток запущен
                                tx_clone
                                    .send(Message::DataReceived(
                                        "🟡 Потік читання запущено".to_string(),
                                    ))
                                    .ok();

                                loop {
                                    // сначала проверим команды на запись (не блокирующе)
                                    match write_rx.try_recv() {
                                        Ok(data_to_write) => {
                                            if let Err(e) = port.write_all(data_to_write.as_bytes())
                                            {
                                                tx_clone
                                                    .send(Message::DataReceived(format!(
                                                        "✗ Помилка запису: {}",
                                                        e
                                                    )))
                                                    .ok();
                                            } else {
                                                // постараемся форсировать отправку из буфера
                                                let _ = port.flush();
                                                tx_clone
                                                    .send(Message::DataReceived(
                                                        "✓ Данні відправлені".to_string(),
                                                    ))
                                                    .ok();
                                            }
                                        }
                                        Err(crossbeam_channel::TryRecvError::Empty) => {
                                            // нет команд — продолжим к чтению
                                        }
                                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                            // отправитель отключился — завершаем поток
                                            tx_clone
                                                .send(Message::DataReceived(
                                                    "! Канал запису закритий".to_string(),
                                                ))
                                                .ok();
                                            break;
                                        }
                                    }

                                    // затем читаем с не очень большим таймаутом (установлен при open)
                                    match port.read(&mut buf) {
                                        Ok(n) if n > 0 => {
                                            let data =
                                                String::from_utf8_lossy(&buf[..n]).to_string();
                                            tx_clone
                                                .send(Message::DataReceived(format!(
                                                    "📦 Отримано {} байт",
                                                    n
                                                )))
                                                .ok();
                                            tx_clone.send(Message::DataReceived(data)).ok();
                                        }
                                        Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                                            // просто ожидаем дальше — это кратковременная пауза
                                            continue;
                                        }
                                        Err(e) => {
                                            tx_clone
                                                .send(Message::DataReceived(format!(
                                                    "! Помилка читання: {}",
                                                    e
                                                )))
                                                .ok();
                                            break;
                                        }
                                        _ => {}
                                    }
                                }

                                // завершение потока — оповестим UI
                                tx_clone
                                    .send(Message::DataReceived(
                                        "🔻 Потік порту завершився".to_string(),
                                    ))
                                    .ok();
                            });

                            self.terminal_output.push_back(format!(
                                "✓ Підключений до {} на {} baud",
                                port_name_clone, self.port_settings.baud_rate
                            ));
                        }
                        Err(e) => {
                            self.terminal_output.push_back(format!(
                                "✗ Помилка підключення до {}: {}",
                                port_name, e
                            ));
                        }
                    }
                }
            }
            Message::DisconnectPort => {
                if let Some(port_name) = &self.port_settings.port_name {
                    self.port_settings.connected = false;
                    self.serial_port = None;
                    self.terminal_output
                        .push_back(format!("⊗ Відключен від {}", port_name));
                }
            }
            Message::RefreshPorts => match available_ports() {
                Ok(ports) => {
                    self.available_ports = ports.into_iter().map(|p| p.port_name).collect();
                    self.terminal_output.push_back(format!(
                        "📋 Знайдено портів: {}",
                        self.available_ports.len()
                    ));
                }
                Err(e) => {
                    self.terminal_output
                        .push_back(format!("✗ Помилка отримання списку портів: {}", e));
                }
            },
            Message::PortsUpdated(ports) => {
                self.available_ports = ports;
                if self.available_ports.is_empty() {
                    self.terminal_output
                        .push_back("! COM порти не знайдені".to_string());
                } else {
                    self.terminal_output.push_back(format!(
                        "📋 Знайдено портів: {}",
                        self.available_ports.len()
                    ));
                }
            }
            Message::StartMonitoring => {
                self.monitoring = true;
                self.terminal_output
                    .push_back("=== Моніторинг запущено ===".to_string());
            }
            Message::StopMonitoring => {
                self.monitoring = false;
                self.terminal_output
                    .push_back("=== Моніторинг зупинено ===".to_string());
            }
            Message::OpenFile => {
                self.log_file_path = Some("example.log".to_string());
                self.terminal_output
                    .push_back("=== Файл відкритий (симуляція) ===".to_string());
            }
            Message::SaveLog => {
                self.terminal_output
                    .push_back("=== Лог збережено (симуляція) ===".to_string());
            }
            Message::DataReceived(data) => {
                self.terminal_output.push_back(format!("↓ {}", data));
                // self.terminal_output.push_back(format!("<- {}", data));
                self.received_bytes += data.len() as u64;
            }
            Message::PortError(error) => {
                if let Some(tx) = &self.tx {
                    tx.send(Message::DataReceived(format!("! Помилка порту: {}", error)))
                        .ok();
                }
                self.terminal_output.push_back(format!("✗ {}", error));
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let nav_bar = row![
            self.nav_button("🖥️ Термінал", WindowState::Terminal),
            self.nav_button("⚙️ Налаштування", WindowState::Settings),
            self.nav_button("≡ Моніторинг", WindowState::Monitor),
            self.nav_button("▣ Файли", WindowState::FileView),
        ]
        .spacing(5)
        .padding([10, 20]);

        let content = match self.current_window {
            WindowState::Terminal => self.terminal_view(),
            WindowState::Settings => self.settings_view(),
            WindowState::Monitor => self.monitor_view(),
            WindowState::FileView => self.file_view(),
        };

        container(column![nav_bar, content].spacing(10))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn nav_button<'a>(&'a self, label: &'a str, window: WindowState) -> Element<'a, Message> {
        let is_active =
            std::mem::discriminant(&self.current_window) == std::mem::discriminant(&window);
        let message = match window {
            WindowState::Terminal => Message::ShowTerminal,
            WindowState::Settings => Message::ShowSettings,
            WindowState::Monitor => Message::ShowMonitor,
            WindowState::FileView => Message::ShowFileView,
        };
        button(text(label).size(if is_active { 16 } else { 14 }))
            .on_press(message)
            .into()
    }

    fn terminal_view(&self) -> Element<Message> {
        let status_text = if self.port_settings.connected {
            text(format!(
                "✓ Підключений до {} ({})",
                self.port_settings
                    .port_name
                    .as_ref()
                    .unwrap_or(&"Unknown".to_string()),
                self.port_settings.baud_rate
            ))
            .size(14)
        } else {
            text("✗ Відключено").size(14)
        };

        let terminal_display = container(scrollable(
            column(
                self.terminal_output
                    .iter()
                    .map(|line| text(line).size(12).into())
                    .collect::<Vec<_>>(),
            )
            .spacing(2)
            .padding(10),
        ))
        .height(Length::FillPortion(3))
        .width(Length::Fill);

        let input_row = row![
            text_input("Введіть команду...✗", &self.input_text)
                .on_input(Message::InputChanged)
                .on_submit(Message::SendData)
                .width(Length::FillPortion(4)),
            button("Відправити")
                .on_press(Message::SendData)
                .width(Length::FillPortion(1)),
        ]
        .spacing(10)
        .padding(10);

        let controls = row![
            button("Очистить").on_press(Message::ClearTerminal),
            text(format!(
                "Відправлено: {} байт | Отримано: {} байт",
                self.sent_bytes, self.received_bytes
            ))
            .size(12),
        ]
        .spacing(10)
        .padding(10);

        column![status_text, terminal_display, input_row, controls,]
            .spacing(10)
            .padding(20)
            .into()
    }

    fn settings_view(&self) -> Element<Message> {
        let port_selection = column![
            text("COM Порт:").size(16),
            pick_list(
                &self.available_ports[..],
                self.port_settings.port_name.as_ref(),
                Message::PortSelected,
            ),
            button("🔄 Оновити список").on_press(Message::RefreshPorts),
        ]
        .spacing(10);

        let baud_selection = column![
            text("Швидкість (baud):").size(16),
            pick_list(
                &self.baud_rates[..],
                Some(self.port_settings.baud_rate),
                Message::BaudRateSelected,
            ),
        ]
        .spacing(10);

        let connection_controls = if self.port_settings.connected {
            button("⊗ Відключитися").on_press(Message::DisconnectPort)
        } else {
            button("⊗ Підключитися").on_press(Message::ConnectPort)
        };

        let additional_settings = container(
            column![
                text("Параметри з'єднання:").size(16),
                text("• Біти данних: 8").size(14),
                text("• Стоп-біти: 1").size(14),
                text("• Четність: None").size(14),
                text("• Керування потоком: None").size(14),
            ]
            .spacing(5),
        )
        .padding(15);

        column![
            text("Налаштування COM порта").size(24),
            port_selection,
            baud_selection,
            connection_controls,
            additional_settings,
        ]
        .spacing(20)
        .padding(20)
        .into()
    }

    fn monitor_view(&self) -> Element<Message> {
        let stats = container(
            column![
                text(format!("↑ Відправлено: {} байт", self.sent_bytes)).size(16),
                text(format!("↓ Отримано: {} байт", self.received_bytes)).size(16),
                text(format!(
                    "≡ Моніторинг: {}",
                    if self.monitoring {
                        "● Активний"
                    } else {
                        "○ Зупинений"
                    }
                ))
                .size(16),
                if self.port_settings.connected {
                    text(format!(
                        "🔗 З'єднання: {} ({})",
                        self.port_settings.port_name.as_ref().unwrap(),
                        self.port_settings.baud_rate
                    ))
                    .size(14)
                } else {
                    text("🔗 З'єднаня: Відключено").size(14)
                },
            ]
            .spacing(10),
        )
        .padding(20);

        let controls = if self.monitoring {
            button("■ Зупинити моніторинг").on_press(Message::StopMonitoring)
        } else {
            button("► Почати моніторинг").on_press(Message::StartMonitoring)
        };

        let chart_placeholder =
            container(text("↑ Здесь будет график трафика\n(TODO: интеграция с plotters)").size(14))
                .padding(30)
                .height(Length::FillPortion(2))
                .width(Length::Fill);

        column![
            text("Моніторинг COM порта").size(24),
            stats,
            controls,
            chart_placeholder,
        ]
        .spacing(20)
        .padding(20)
        .into()
    }

    fn file_view(&self) -> Element<Message> {
        let file_info = container(if let Some(path) = &self.log_file_path {
            text(format!("📄 Теперішній файл: {}", path)).size(14)
        } else {
            text("📄 Файл не вибрано").size(14)
        })
        .padding(15);

        let file_controls = row![
            button("▣ Відкрити файл").on_press(Message::OpenFile),
            button("⎙ Зберігти лог").on_press(Message::SaveLog),
        ]
        .spacing(10);

        let log_preview = container(scrollable(
            column(
                self.terminal_output
                    .iter()
                    .take(10)
                    .map(|line| text(line).size(12).into())
                    .collect::<Vec<_>>(),
            )
            .spacing(2)
            .padding(10),
        ))
        .height(Length::FillPortion(2))
        .width(Length::Fill);

        column![
            text("Робота з файлами").size(24),
            file_info,
            file_controls,
            text("Попередній перегляд лога:").size(16),
            log_preview,
        ]
        .spacing(20)
        .padding(20)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        if let Some(rx) = &self.rx {
            subscription::from_recipe(ComReceiver {
                rx: Arc::new(rx.clone()),
            })
        } else {
            Subscription::none()
        }
    }
    fn window_title(&self) -> String {
        let status = if self.port_settings.connected {
            format!(
                " - Підключений до {}",
                self.port_settings
                    .port_name
                    .as_ref()
                    .unwrap_or(&"Unknown".to_string())
            )
        } else {
            " - Відключений".to_string()
        };

        match self.current_window {
            WindowState::Terminal => format!("COM Terminal - Термінал{}", status),
            WindowState::Settings => format!("COM Terminal - Налаштунки{}", status),
            WindowState::Monitor => format!("COM Terminal - Моніторинг{}", status),
            WindowState::FileView => format!("COM Terminal - Файлы{}", status),
        }
    }
}

pub fn main() -> iced::Result {
    iced::application("COM Terminal", ComTerminal::update, ComTerminal::view)
        .theme(|_| Theme::Dark)
        // Emoji fallback (если нужно можно вызывать emoji_font() в виджетах)
        // .font(include_bytes!("../../fonts/NotoColorEmoji.ttf").as_slice())
        // fallback монохромный символ/emoji шрифт (seguisym.ttf) — добавьте в папку fonts
        .font(include_bytes!("../../fonts/seguisym.ttf").as_slice())
        // Основной шрифт (старые .font(...) с сырыми байтами можно оставить)
        .font(include_bytes!("../../fonts/jetbrains-mono.regular.ttf").as_slice())
        // используем функцию вместо проблемного `Font::External { ... }`
        // .default_font(jetbrains_mono())
        .subscription(ComTerminal::subscription)
        .run_with(|| (ComTerminal::new(), iced::Task::none()))
}
