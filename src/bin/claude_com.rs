#![windows_subsystem = "windows"]
use iced::executor;
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Application, Command, Element, Renderer, Settings, Subscription, Theme};
use serialport::{available_ports, SerialPort};
use std::collections::VecDeque;
use std::io::Write;
use std::io::{self, BufReader};
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
    SendResult(String),
    PortError(String),
}

#[derive(Debug, Clone, Default)]
pub enum WindowState {
    #[default] // Указываем, что Terminal - это значение по умолчанию
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

    // Terminal data
    input_text: String,
    terminal_output: VecDeque<String>,

    // Settings
    port_settings: PortSettings,
    available_ports: Vec<String>,
    baud_rates: Vec<u32>,

    // Monitor
    monitoring: bool,
    received_bytes: u64,
    sent_bytes: u64,

    // File
    log_file_path: Option<String>,

    // Serial port
    serial_port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
}

impl ComTerminal {
    fn new() -> (Self, Task<Message>) {
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

        // Добавляем приветственное сообщение
        terminal
            .terminal_output
            .push_back("=== COM Terminal запущен ===".to_string());
        terminal
            .terminal_output
            .push_back("Загружаем список COM портов...".to_string());

        (
            terminal,
            Task::perform(get_available_ports(), Message::PortsUpdated),
        )
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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Навигация
            Message::ShowTerminal => {
                self.current_window = WindowState::Terminal;
                Task::none()
            }
            Message::ShowSettings => {
                self.current_window = WindowState::Settings;
                Task::none()
            }
            Message::ShowMonitor => {
                self.current_window = WindowState::Monitor;
                Task::none()
            }
            Message::ShowFileView => {
                self.current_window = WindowState::FileView;
                Task::none()
            }

            // Terminal
            Message::InputChanged(text) => {
                self.input_text = text;
                Task::none()
            }
            // Обработка сообщения Message::SendData в update
            Message::SendData => {
                if !self.input_text.is_empty() && self.port_settings.connected {
                    let data = self.input_text.clone();
                    self.terminal_output.push_back(format!(">>> {}", data));
                    self.sent_bytes += data.len() as u64;

                    self.input_text.clear();

                    if let Some(port) = &self.serial_port {
                        let port_clone = port.clone();

                        // Запускаем асинхронную задачу для отправки данных
                        return Task::perform(send_data_to_port(port_clone, data), |result| {
                            match result {
                                Ok(message) => Message::SendResult(message),
                                Err(e) => Message::PortError(e.to_string()),
                            }
                        });
                    }
                }

                // Если отправка невозможна, просто ничего не делаем
                Task::none()
            }
            Message::SendResult(message) => {
                self.terminal_output.push_back(message);
                Task::none()
            }

            Message::ClearTerminal => {
                self.terminal_output.clear();
                self.terminal_output
                    .push_back("=== Терминал очищен ===".to_string());
                Task::none()
            }

            // Settings
            Message::PortSelected(port) => {
                self.port_settings.port_name = Some(port);
                Task::none()
            }
            Message::BaudRateSelected(rate) => {
                self.port_settings.baud_rate = rate;
                Task::none()
            }
            Message::ConnectPort => {
                if let Some(port_name) = &self.port_settings.port_name {
                    match serialport::new(port_name, self.port_settings.baud_rate)
                        .timeout(Duration::from_millis(1000))
                        .open()
                    {
                        Ok(port) => {
                            self.port_settings.connected = true;
                            self.serial_port = Some(Arc::new(Mutex::new(port)));
                            self.terminal_output.push_back(format!(
                                "✅ Подключен к {} на {} baud",
                                port_name, self.port_settings.baud_rate
                            ));
                            // Запускаем асинхронную задачу для чтения порта
                            let port_clone = self.serial_port.clone().unwrap();
                            return Task::perform(
                                read_from_port(port_clone),
                                |result| match result {
                                    Ok(data) => Message::DataReceived(data),
                                    Err(e) => Message::PortError(e.to_string()),
                                },
                            );
                        }
                        Err(e) => {
                            self.terminal_output
                                .push_back(format!("❌ Ошибка подключения к {}: {}", port_name, e));
                        }
                    }
                }
                Task::none()
            }
            Message::DisconnectPort => {
                if let Some(port_name) = &self.port_settings.port_name {
                    self.port_settings.connected = false;
                    self.serial_port = None;
                    self.terminal_output
                        .push_back(format!("🔌 Отключен от {}", port_name));
                }
                Task::none()
            }
            Message::RefreshPorts => Task::perform(get_available_ports(), Message::PortsUpdated),
            Message::PortsUpdated(ports) => {
                self.available_ports = ports;
                if self.available_ports.is_empty() {
                    self.terminal_output
                        .push_back("⚠️ COM порты не найдены".to_string());
                } else {
                    self.terminal_output
                        .push_back(format!("📋 Найдено портов: {}", self.available_ports.len()));
                }
                Task::none()
            }

            // Monitor
            Message::StartMonitoring => {
                self.monitoring = true;
                self.terminal_output
                    .push_back("=== Мониторинг запущен ===".to_string());
                Task::none()
            }
            Message::StopMonitoring => {
                self.monitoring = false;
                self.terminal_output
                    .push_back("=== Мониторинг остановлен ===".to_string());
                Task::none()
            }

            // File
            Message::OpenFile => {
                // TODO: Implement file dialog with rfd
                self.log_file_path = Some("example.log".to_string());
                self.terminal_output
                    .push_back("=== Файл открыт (симуляция) ===".to_string());
                Task::none()
            }
            Message::SaveLog => {
                // TODO: Save terminal output to file
                self.terminal_output
                    .push_back("=== Лог сохранен (симуляция) ===".to_string());
                Task::none()
            }
            Message::DataReceived(data) => {
                self.terminal_output.push_back(format!("<- {}", data));

                // Запускаем асинхронную задачу для следующего чтения
                let port_clone = self.serial_port.clone().unwrap();
                return Task::perform(read_from_port(port_clone), |result| match result {
                    Ok(data) => Message::DataReceived(data),
                    Err(e) => Message::PortError(e.to_string()),
                });
            }
            Message::PortError(error) => {
                self.terminal_output.push_back(format!("❌ {}", error));
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let nav_bar = row![
            self.nav_button("🖥️ Терминал", WindowState::Terminal, Message::ShowTerminal),
            self.nav_button("⚙️ Настройки", WindowState::Settings, Message::ShowSettings),
            self.nav_button("📊 Мониторинг", WindowState::Monitor, Message::ShowMonitor),
            self.nav_button("📁 Файлы", WindowState::FileView, Message::ShowFileView),
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

    fn nav_button<'a>(
        &'a self,
        label: &'a str,
        window: WindowState,
        message: Message,
    ) -> Element<'a, Message> {
        let is_active =
            std::mem::discriminant(&self.current_window) == std::mem::discriminant(&window);

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
                Message::PortSelected
            ),
            button("🔄 Обновить список").on_press(Message::RefreshPorts),
        ]
        .spacing(10);

        let baud_selection = column![
            text("Скорость (baud):").size(16),
            pick_list(
                &self.baud_rates[..],
                Some(self.port_settings.baud_rate),
                Message::BaudRateSelected
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
                    .take(10) // Показываем последние 10 строк
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
    // В вашем impl ComTerminal
    fn subscription(&self) -> iced::Subscription<Message> {
        if self.port_settings.connected {
            // Здесь вы должны создать и вернуть подписку на чтение порта
            // Например: iced::Subscription::from_recipe(MyPortReader { ... })
            todo!("Implement port reader subscription")
        } else {
            // Если порт не подключен, возвращаем пустую подписку
            iced::Subscription::none()
        }
    }
}

// Асинхронная функция для получения списка портов
async fn get_available_ports() -> Vec<String> {
    match available_ports() {
        Ok(ports) => {
            let mut port_names = Vec::new();
            for port in ports {
                port_names.push(port.port_name);
            }
            port_names
        }
        Err(_) => {
            // Если не удалось получить порты, возвращаем пустой список
            Vec::new()
        }
    }
}
// Асинхронная функция для чтения данных
async fn read_from_port(
    port: Arc<Mutex<Box<dyn serialport::SerialPort>>>,
) -> Result<String, serialport::Error> {
    let mut port = port.lock().unwrap();
    let mut buffer: Vec<u8> = vec![0; 1024];

    loop {
        match port.read(buffer.as_mut_slice()) {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    let data = String::from_utf8_lossy(&buffer[..bytes_read]);
                    return Ok(data.to_string()); // Возвращаем сразу
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::TimedOut {
                    return Err(serialport::Error::new(
                        serialport::ErrorKind::Io(e.kind()),
                        "Ошибка чтения порта",
                    ));
                }
            }
        }
    }
}
// Асинхронная функция для отправки данных
async fn send_data_to_port(
    port: Arc<Mutex<Box<dyn serialport::SerialPort>>>,
    data: String,
) -> Result<String, serialport::Error> {
    let mut port = port.lock().unwrap();
    match port.write_all(data.as_bytes()) {
        Ok(_) => Ok(format!("✓ Данные отправлены: {}", data)),
        Err(e) => Err(serialport::Error::new(
            serialport::ErrorKind::Io(e.kind()),
            "Ошибка отправки данных",
        )),
    }
}
pub fn main() -> iced::Result {
    iced::application(
        "COM Terminal",      // Заголовок вашего приложения
        ComTerminal::update, // Функция update
        ComTerminal::view,   // Функция view
    )
    .subscription(ComTerminal::subscription) // Добавляем подписку
    .run()
}
