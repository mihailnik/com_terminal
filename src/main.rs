//! A simple terminal application for serial port communication using the iced framework.
//! This code is updated to work with a recent version of the `iced` crate`.

// This attribute prevents the console window from appearing on Windows
#![windows_subsystem = "windows"]

use iced::executor;
use iced::widget::{
    button, checkbox, container, pick_list, radio, scrollable, text, text_input, Column, Row,
};
use iced::{Alignment, Application, Command, Element, Length, Subscription, Theme};
use serialport::{available_ports, DataBits, Parity, SerialPort, StopBits};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;

// We need to define a unique ID for our subscription. This can be any hashable value.
// A simple struct works well.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SerialPortSubscriptionId;

/// The main application state.
#[derive(Debug)]
struct Terminal {
    // Connection state
    log: Vec<String>,
    port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
    show_received_prefix: bool, // Новая настройка для префикса

    // UI state for managing ports
    available_ports: Vec<String>,
    selected_port: Option<String>,
    baud_rates: Vec<u32>,
    selected_baud_rate: Option<u32>,
    data_bits: DataBits,
    stop_bits: StopBits,
    parity: Parity,

    // UI state for sending and saving data
    input_text: String,
    file_path: String,
    scroll_id: scrollable::Id,
}

/// The messages our application can handle.
#[derive(Debug, Clone)]
enum Message {
    // UI-related messages
    PortSelected(String),
    BaudRateSelected(u32),
    DataBitsSelected(DataBits),
    StopBitsSelected(DataBits),
    ParitySelected(Parity),
    ConnectClicked,
    DisconnectClicked,
    ClearLogClicked,
    SaveLogClicked,
    FilePathChanged(String),
    SendFromFileClicked,
    InputChanged(String),
    InputSubmitted,
    ToggleReceivedPrefix(bool), // Новое сообщение для флажка

    // Serial port related messages
    PortsFound(Result<Vec<String>, String>),
    SerialDataReceived(String),
    PortConnected(Result<Arc<Mutex<Box<dyn SerialPort>>>, String>),
    SerialError(String),

    // Helper message
    NoOp,
    // New message for periodic port scanning
    ScanPorts,
}

impl Application for Terminal {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    /// Initialize the application.
    fn new(_flags: ()) -> (Terminal, Command<Message>) {
        let baud_rates = vec![
            110, 300, 600, 1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200, 128000,
            256000,
        ];

        (
            Terminal {
                log: vec!["Ожидание подключения...".to_string()],
                port: None,
                show_received_prefix: true, // По умолчанию префикс включен
                available_ports: Vec::new(),
                selected_port: None,
                baud_rates: baud_rates.clone(),
                selected_baud_rate: Some(115200), // Установлено 115200 по умолчанию
                data_bits: DataBits::Eight,
                stop_bits: StopBits::One,
                parity: Parity::None,
                input_text: String::new(),
                file_path: String::new(),
                scroll_id: scrollable::Id::new("log_scrollable"),
            },
            // We start the port search right at launch
            Command::perform(find_ports(), Message::PortsFound),
        )
    }

    /// The application's title.
    fn title(&self) -> String {
        String::from("COM Terminal")
    }

    /// We define the application's theme.
    fn theme(&self) -> Self::Theme {
        Self::Theme::Dark
    }

    /// Handle incoming messages and update the application state.
    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PortsFound(Ok(ports)) => {
                // Check if the list of ports has changed
                if self.available_ports != ports {
                    self.available_ports = ports;
                    self.selected_port = self.available_ports.first().cloned();
                }
                Command::none()
            }
            Message::PortsFound(Err(e)) => {
                self.log.push(format!("Ошибка при поиске портов: {}", e));
                Command::none()
            }
            Message::PortSelected(port) => {
                self.selected_port = Some(port);
                Command::none()
            }
            Message::BaudRateSelected(baud) => {
                self.selected_baud_rate = Some(baud);
                Command::none()
            }
            Message::DataBitsSelected(bits) => {
                self.data_bits = bits;
                Command::none()
            }
            Message::StopBitsSelected(bits) => {
                self.stop_bits = bits;
                Command::none()
            }
            Message::ParitySelected(parity) => {
                self.parity = parity;
                Command::none()
            }
            Message::ConnectClicked => {
                let selected_port_name = self.selected_port.clone();
                let selected_baud_rate = self.selected_baud_rate;
                let data_bits = self.data_bits;
                let stop_bits = self.stop_bits;
                let parity = self.parity;

                if let (Some(port_name), Some(baud_rate)) = (selected_port_name, selected_baud_rate)
                {
                    self.log
                        .push(format!("Попытка подключения к {}...", port_name));

                    return Command::perform(
                        async move {
                            // Добавлена явная установка таймаута
                            serialport::new(&port_name, baud_rate)
                                .data_bits(data_bits)
                                .stop_bits(stop_bits)
                                .parity(parity)
                                .timeout(Duration::from_millis(1000)) // Установлен таймаут в 1000 мс
                                .open()
                                .map(|port| Arc::new(Mutex::new(port)))
                                .map_err(|e| e.to_string())
                        },
                        Message::PortConnected,
                    );
                }
                Command::none()
            }
            Message::DisconnectClicked => {
                self.port = None;
                self.log.push("Соединение закрыто.".to_string());
                Command::none()
            }
            Message::ClearLogClicked => {
                self.log.clear();
                self.log.push("Лог очищен.".to_string());
                Command::none()
            }
            Message::SaveLogClicked => {
                let log_content = self.log.join("\n");
                let result = std::fs::write("com_log.txt", log_content);
                match result {
                    Ok(_) => self.log.push("Лог сохранён в com_log.txt".to_string()),
                    Err(e) => self.log.push(format!("Ошибка сохранения: {}", e)),
                }
                Command::none()
            }
            Message::FilePathChanged(path) => {
                self.file_path = path;
                Command::none()
            }
            Message::SendFromFileClicked => {
                let file_path = self.file_path.clone();
                if let Some(port_mutex) = &self.port {
                    let port_mutex = port_mutex.clone();
                    self.log
                        .push(format!("< Отправка данных из файла: {}", file_path));
                    return Command::perform(
                        async move {
                            let mut file = File::open(&file_path).map_err(|e| e.to_string())?;
                            let mut contents = Vec::new();
                            file.read_to_end(&mut contents).map_err(|e| e.to_string())?;

                            let mut port = port_mutex.lock().unwrap();
                            port.write_all(&contents).map_err(|e| e.to_string())?;

                            Ok(())
                        },
                        |result: Result<(), String>| match result {
                            Ok(_) => {
                                Message::SerialDataReceived("Отправка файла завершена.".to_string())
                            }
                            Err(e) => Message::SerialError(format!("Ошибка отправки файла: {}", e)),
                        },
                    );
                } else {
                    self.log.push("Ошибка: Порт не открыт.".to_string());
                }
                Command::none()
            }
            Message::InputChanged(value) => {
                self.input_text = value;
                Command::none()
            }
            Message::InputSubmitted => {
                if let Some(port_mutex) = &self.port {
                    let input_to_send = self.input_text.clone();
                    self.log.push(format!("< {}", input_to_send));
                    self.input_text = String::new(); // Clear the input field

                    let port_mutex = port_mutex.clone();
                    return Command::perform(
                        async move {
                            let mut port = port_mutex.lock().unwrap();
                            let result = port.write_all(input_to_send.as_bytes());
                            match result {
                                Ok(_) => Ok(()),
                                Err(e) => Err(e.to_string()),
                            }
                        },
                        |result: Result<(), String>| match result {
                            Ok(_) => Message::NoOp, // The subscription will handle received data
                            Err(e) => Message::SerialError(e),
                        },
                    );
                } else {
                    self.log.push("Ошибка: Порт не открыт.".to_string());
                }
                Command::none()
            }
            Message::SerialDataReceived(data) => {
                if self.show_received_prefix {
                    self.log.push(format!("> {}", data));
                } else {
                    self.log.push(data);
                }
                scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
            }
            Message::PortConnected(Ok(port_arc)) => {
                self.port = Some(port_arc); // We save the port in the state
                self.log.push("Соединение успешно установлено.".to_string());
                Command::none()
            }
            Message::PortConnected(Err(e)) => {
                self.log.push(format!("Ошибка подключения: {}", e));
                self.port = None;
                Command::none()
            }
            Message::SerialError(e) => {
                self.log.push(format!("Ошибка COM-порта: {}", e));
                self.port = None;
                Command::none()
            }
            Message::ToggleReceivedPrefix(checked) => {
                self.show_received_prefix = checked;
                Command::none()
            }
            // A new message handler to trigger the port scan command.
            Message::ScanPorts => Command::perform(find_ports(), Message::PortsFound),
            Message::NoOp => Command::none(),
        }
    }

    /// Define the application's subscriptions.
    fn subscription(&self) -> Subscription<Message> {
        // We start the subscription only if there is an open port
        let serial_subscription = if let Some(port_arc) = &self.port {
            let port_arc = port_arc.clone();
            iced::subscription::unfold(
                SerialPortSubscriptionId,
                (port_arc, [0u8; 1024]),
                move |(port_arc, mut buf)| {
                    async move {
                        let result = task::spawn_blocking(move || {
                            let read_result = {
                                let mut port = port_arc.lock().unwrap();
                                port.read(&mut buf)
                            };
                            (read_result, port_arc, buf)
                        })
                        .await
                        .unwrap();

                        let (read_result, port_arc, buf) = result;

                        match read_result {
                            Ok(bytes_read) => {
                                if bytes_read > 0 {
                                    let received_data =
                                        String::from_utf8_lossy(&buf[..bytes_read]).to_string();
                                    (
                                        Some(Message::SerialDataReceived(received_data)),
                                        (port_arc, buf),
                                    )
                                } else {
                                    (Some(Message::NoOp), (port_arc, buf))
                                }
                            }
                            Err(e) => {
                                // Обрабатываем ошибку таймаута отдельно, чтобы не отключаться
                                if e.kind() == ErrorKind::TimedOut {
                                    (Some(Message::NoOp), (port_arc, buf))
                                } else {
                                    // Все остальные ошибки считаем критическими и отключаемся
                                    (Some(Message::SerialError(e.to_string())), (port_arc, buf))
                                }
                            }
                        }
                    }
                },
            )
            .map(|message_option| message_option.unwrap_or(Message::NoOp))
        } else {
            Subscription::none()
        };

        // This subscription triggers a `ScanPorts` message every 5 seconds.
        // The `update` method will then handle this message and perform the port search.
        let port_scan_subscription =
            iced::time::every(Duration::from_secs(5)).map(|_| Message::ScanPorts);

        Subscription::batch(vec![serial_subscription, port_scan_subscription])
    }

    /// The application's main view.
    fn view(&self) -> Element<Message> {
        // Title and controls
        let controls = Row::new()
            .spacing(10)
            .align_items(Alignment::Center)
            .push(pick_list(
                self.available_ports.clone(),
                self.selected_port.as_ref(),
                Message::PortSelected,
            ))
            .push(pick_list(
                self.baud_rates.clone(),
                self.selected_baud_rate.as_ref(),
                Message::BaudRateSelected,
            ))
            .push(
                // Connect/disconnect button
                if self.port.is_some() {
                    button("Закрыть").on_press(Message::DisconnectClicked)
                } else {
                    button("Открыть").on_press(Message::ConnectClicked)
                },
            );

        // Radio buttons for port settings
        let port_settings = Row::new()
            .spacing(10)
            .align_items(Alignment::Center)
            .push(text("Биты данных:").size(14))
            .push(
                radio(
                    "5",
                    DataBits::Five,
                    Some(self.data_bits),
                    Message::DataBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "6",
                    DataBits::Six,
                    Some(self.data_bits),
                    Message::DataBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "7",
                    DataBits::Seven,
                    Some(self.data_bits),
                    Message::DataBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "8",
                    DataBits::Eight,
                    Some(self.data_bits),
                    Message::DataBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(text("Стоп-биты:").size(14))
            .push(
                radio(
                    "1",
                    StopBits::One,
                    Some(self.stop_bits),
                    Message::StopBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "2",
                    StopBits::Two,
                    Some(self.stop_bits),
                    Message::StopBitsSelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(text("Четность:").size(14))
            .push(
                radio(
                    "Нет",
                    Parity::None,
                    Some(self.parity),
                    Message::ParitySelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "Нечет.",
                    Parity::Odd,
                    Some(self.parity),
                    Message::ParitySelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            )
            .push(
                radio(
                    "Четн.",
                    Parity::Even,
                    Some(self.parity),
                    Message::ParitySelected,
                )
                .text_size(14)
                .size(18), // <-- Уменьшаем размер самой радиокнопки
            );

        // Log window and control buttons
        let log_content = self.log.iter().fold(
            Column::new().spacing(5),
            |column, line| column.push(text(line.clone()).size(14)), // Уменьшенный размер шрифта
        );

        let log_display = container(scrollable(log_content).id(self.scroll_id.clone()))
            .padding(10)
            .style(iced::theme::Container::Box)
            .height(Length::FillPortion(2)) // Исправлена ошибка: 2.0 заменено на 2
            .width(Length::Fill);

        let log_buttons_and_settings = Row::new()
            .spacing(10)
            .align_items(Alignment::Center)
            .push(button("Сохранить").on_press(Message::SaveLogClicked))
            .push(button("Очистить").on_press(Message::ClearLogClicked))
            .push(
                checkbox("Показывать префикс", self.show_received_prefix)
                    .on_toggle(Message::ToggleReceivedPrefix)
                    .text_size(14),
            );

        // Text input and send
        let input_row = Row::new()
            .spacing(10)
            .align_items(Alignment::Center)
            .push(
                text_input("Введите команду...", &self.input_text)
                    .on_input(Message::InputChanged)
                    .on_submit(Message::InputSubmitted),
            )
            .push(button("Отправить").on_press(Message::InputSubmitted));

        // Send file
        let file_input_row = Row::new()
            .spacing(10)
            .align_items(Alignment::Center)
            .push(button("Открыть файл").on_press(Message::SendFromFileClicked)) // In a real application, this would be a file dialog
            .push(text_input("Путь к файлу...", &self.file_path).on_input(Message::FilePathChanged))
            .push(button("Отправить файл").on_press(Message::SendFromFileClicked));

        // Main layout
        let content = Column::new()
            .align_items(Alignment::Center)
            .spacing(10)
            .padding(10)
            .push(controls)
            .push(port_settings)
            .push(log_display)
            .push(log_buttons_and_settings)
            .push(input_row)
            .push(file_input_row);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// A simple function to find available serial ports.
async fn find_ports() -> Result<Vec<String>, String> {
    available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .map_err(|e| e.to_string())
}

// The main function that runs the application.
fn main() -> iced::Result {
    Terminal::run(iced::Settings::default())
}
