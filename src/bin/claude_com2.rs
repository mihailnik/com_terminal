#![windows_subsystem = "windows"]

use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Theme};
use serialport::{available_ports, SerialPort};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
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

    // Internal messages
    Tick,
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
            baud_rate: 9600,
            connected: false,
        }
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
        };

        terminal
            .terminal_output
            .push_back("=== COM Terminal запущен ===".to_string());
        terminal
            .terminal_output
            .push_back("Загружаем список COM портов...".to_string());

        // Initial loading of ports
        match available_ports() {
            Ok(ports) => {
                terminal.available_ports = ports.into_iter().map(|p| p.port_name).collect();
                if terminal.available_ports.is_empty() {
                    terminal
                        .terminal_output
                        .push_back("⚠️ COM порты не найдены".to_string());
                } else {
                    terminal.terminal_output.push_back(format!(
                        "📋 Найдено портов: {}",
                        terminal.available_ports.len()
                    ));
                }
            }
            Err(e) => {
                terminal
                    .terminal_output
                    .push_back(format!("❌ Ошибка получения списка портов: {}", e));
            }
        }

        terminal
    }

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
                                    .push_back(format!("✓ Данные отправлены"));
                            }
                            Err(e) => {
                                self.terminal_output
                                    .push_back(format!("❌ Ошибка отправки данных: {}", e));
                            }
                        }
                    }
                    self.input_text.clear();
                }
            }
            Message::ClearTerminal => {
                self.terminal_output.clear();
                self.terminal_output
                    .push_back("=== Терминал очищен ===".to_string());
            }
            Message::PortSelected(port) => {
                self.port_settings.port_name = Some(port);
            }
            Message::BaudRateSelected(rate) => {
                self.port_settings.baud_rate = rate;
            }
            Message::ConnectPort => {
                if let Some(port_name) = &self.port_settings.port_name {
                    match serialport::new(port_name, self.port_settings.baud_rate)
                        .timeout(Duration::from_millis(100))
                        .open()
                    {
                        Ok(port) => {
                            self.port_settings.connected = true;
                            self.serial_port = Some(Arc::new(Mutex::new(port)));
                            self.terminal_output.push_back(format!(
                                "✅ Подключен к {} на {} baud",
                                port_name, self.port_settings.baud_rate
                            ));
                        }
                        Err(e) => {
                            self.terminal_output
                                .push_back(format!("❌ Ошибка подключения к {}: {}", port_name, e));
                        }
                    }
                }
            }
            Message::DisconnectPort => {
                if let Some(port_name) = &self.port_settings.port_name {
                    self.port_settings.connected = false;
                    self.serial_port = None;
                    self.terminal_output
                        .push_back(format!("🔌 Отключен от {}", port_name));
                }
            }
            Message::RefreshPorts => match available_ports() {
                Ok(ports) => {
                    self.available_ports = ports.into_iter().map(|p| p.port_name).collect();
                    self.terminal_output
                        .push_back(format!("📋 Найдено портов: {}", self.available_ports.len()));
                }
                Err(e) => {
                    self.terminal_output
                        .push_back(format!("❌ Ошибка получения списка портов: {}", e));
                }
            },
            Message::PortsUpdated(ports) => {
                self.available_ports = ports;
                if self.available_ports.is_empty() {
                    self.terminal_output
                        .push_back("⚠️ COM порты не найдены".to_string());
                } else {
                    self.terminal_output
                        .push_back(format!("📋 Найдено портов: {}", self.available_ports.len()));
                }
            }
            Message::StartMonitoring => {
                self.monitoring = true;
                self.terminal_output
                    .push_back("=== Мониторинг запущен ===".to_string());
            }
            Message::StopMonitoring => {
                self.monitoring = false;
                self.terminal_output
                    .push_back("=== Мониторинг остановлен ===".to_string());
            }
            Message::OpenFile => {
                self.log_file_path = Some("example.log".to_string());
                self.terminal_output
                    .push_back("=== Файл открыт (симуляция) ===".to_string());
            }
            Message::SaveLog => {
                self.terminal_output
                    .push_back("=== Лог сохранен (симуляция) ===".to_string());
            }
            Message::DataReceived(data) => {
                self.terminal_output.push_back(format!("<- {}", data));
                self.received_bytes += data.len() as u64;
            }
            Message::PortError(error) => {
                self.terminal_output.push_back(format!("❌ {}", error));
            }
            Message::Tick => {
                if let Some(port) = &self.serial_port {
                    let mut port_lock = port.lock().unwrap();
                    let mut buffer = [0; 1024];

                    match port_lock.read(&mut buffer) {
                        Ok(bytes_read) => {
                            if bytes_read > 0 {
                                let data =
                                    String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                                self.terminal_output.push_back(format!("<- {}", data));
                                self.received_bytes += data.len() as u64;
                            }
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                            // Do nothing on timeout
                        }
                        Err(e) => {
                            self.terminal_output
                                .push_back(format!("❌ Ошибка чтения из порта: {}", e));
                        }
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let nav_bar = row![
            self.nav_button("🖥️ Терминал", WindowState::Terminal),
            self.nav_button("⚙️ Настройки", WindowState::Settings),
            self.nav_button("📊 Мониторинг", WindowState::Monitor),
            self.nav_button("📁 Файлы", WindowState::FileView),
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
                "✅ Подключен к {} ({})",
                self.port_settings
                    .port_name
                    .as_ref()
                    .unwrap_or(&"Unknown".to_string()),
                self.port_settings.baud_rate
            ))
            .size(14)
        } else {
            text("❌ Отключен").size(14)
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
            text_input("Введите команду...", &self.input_text)
                .on_input(Message::InputChanged)
                .on_submit(Message::SendData)
                .width(Length::FillPortion(4)),
            button("Отправить")
                .on_press(Message::SendData)
                .width(Length::FillPortion(1)),
        ]
        .spacing(10)
        .padding(10);

        let controls = row![
            button("Очистить").on_press(Message::ClearTerminal),
            text(format!(
                "Отправлено: {} байт | Получено: {} байт",
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
            button("🔄 Обновить список").on_press(Message::RefreshPorts),
        ]
        .spacing(10);

        let baud_selection = column![
            text("Скорость (baud):").size(16),
            pick_list(
                &self.baud_rates[..],
                Some(self.port_settings.baud_rate),
                Message::BaudRateSelected,
            ),
        ]
        .spacing(10);

        let connection_controls = if self.port_settings.connected {
            button("🔌 Отключиться").on_press(Message::DisconnectPort)
        } else {
            button("🔌 Подключиться").on_press(Message::ConnectPort)
        };

        let additional_settings = container(
            column![
                text("Параметры соединения:").size(16),
                text("• Биты данных: 8").size(14),
                text("• Стоп-биты: 1").size(14),
                text("• Четность: None").size(14),
                text("• Управление потоком: None").size(14),
            ]
            .spacing(5),
        )
        .padding(15);

        column![
            text("Настройки COM порта").size(24),
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
                text(format!("📤 Отправлено: {} байт", self.sent_bytes)).size(16),
                text(format!("📥 Получено: {} байт", self.received_bytes)).size(16),
                text(format!(
                    "📊 Мониторинг: {}",
                    if self.monitoring {
                        "🟢 Активен"
                    } else {
                        "🔴 Остановлен"
                    }
                ))
                .size(16),
                if self.port_settings.connected {
                    text(format!(
                        "🔗 Соединение: {} ({})",
                        self.port_settings.port_name.as_ref().unwrap(),
                        self.port_settings.baud_rate
                    ))
                    .size(14)
                } else {
                    text("🔗 Соединение: Отключено").size(14)
                },
            ]
            .spacing(10),
        )
        .padding(20);

        let controls = if self.monitoring {
            button("⏹️ Остановить мониторинг").on_press(Message::StopMonitoring)
        } else {
            button("▶️ Начать мониторинг").on_press(Message::StartMonitoring)
        };

        let chart_placeholder = container(
            text("📈 Здесь будет график трафика\n(TODO: интеграция с plotters)").size(14),
        )
        .padding(30)
        .height(Length::FillPortion(2))
        .width(Length::Fill);

        column![
            text("Мониторинг COM порта").size(24),
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
            text(format!("📄 Текущий файл: {}", path)).size(14)
        } else {
            text("📄 Файл не выбран").size(14)
        })
        .padding(15);

        let file_controls = row![
            button("📁 Открыть файл").on_press(Message::OpenFile),
            button("💾 Сохранить лог").on_press(Message::SaveLog),
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
            text("Работа с файлами").size(24),
            file_info,
            file_controls,
            text("Предварительный просмотр лога:").size(16),
            log_preview,
        ]
        .spacing(20)
        .padding(20)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.port_settings.connected {
            return iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick);
        }
        Subscription::none()
    }

    fn title(&self) -> String {
        let status = if self.port_settings.connected {
            format!(
                " - Подключен к {}",
                self.port_settings
                    .port_name
                    .as_ref()
                    .unwrap_or(&"Unknown".to_string())
            )
        } else {
            " - Отключен".to_string()
        };

        match self.current_window {
            WindowState::Terminal => format!("COM Terminal - Терминал{}", status),
            WindowState::Settings => format!("COM Terminal - Настройки{}", status),
            WindowState::Monitor => format!("COM Terminal - Мониторинг{}", status),
            WindowState::FileView => format!("COM Terminal - Файлы{}", status),
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

pub fn main() -> iced::Result {
    iced::application("COM Terminal", ComTerminal::update, ComTerminal::view)
        .theme(ComTerminal::theme)
        .subscription(ComTerminal::subscription)
        .run_with(|| (ComTerminal::new(), iced::Task::none()))
}
