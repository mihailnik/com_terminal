//! A simple serial port terminal built with the Iced GUI library.
//! This application allows users to connect to a serial port, send commands,
//! and receive data. It also includes a basic plot to visualize incoming data.

// Imports for the Iced GUI library and asynchronous operations.
use iced::advanced::subscription;
use iced::command::Command;
use iced::futures::{self, stream::BoxStream, StreamExt};
use iced::widget::{
    button, column, container, horizontal_space, row, text, text_input, vertical_space,
};
use iced::{
    executor, Alignment, Application, Element, Length, Renderer, Settings, Subscription, Theme,
};
// Imports for serial port communication.
use serialport::{DataBits, Parity, StopBits};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::{self as tokio_serial, SerialPortBuilderExt};

// Import for plotting.
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters_iced::{Chart, ChartWidget};

// --- Constants ---
const BAUD_RATES: [u32; 10] = [110, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600];
const MAX_DATA_POINTS: usize = 100;
const BAUD_DEFAULT: u32 = 9600;

// --- Application State ---
#[derive(Debug, Default)]
struct ComApp {
    serial_port: Option<Arc<Mutex<Box<dyn serialport::SerialPort>>>>,
    tx: Option<mpsc::Sender<String>>,
    rx_task: Option<tokio::task::JoinHandle<()>>,
    serial_port_name: Option<String>,
    baud_rate: u32,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: StopBits,
    buffer: String,
    input_text: String,
    is_connected: bool,
    ports_names: Vec<String>,
    errors: Vec<String>,
    data_points: Vec<(f64, f64)>,
    data_counter: usize,
}

// --- Application Messages ---
#[derive(Debug, Clone)]
enum Message {
    Connect,
    Disconnect,
    PortSelected(String),
    BaudRateSelected(u32),
    BaudRateTextChanged(String),
    InputTextChanged(String),
    Send,
    SerialDataReceived(Vec<u8>),
    ListPorts,
    PortListReceived(Vec<String>),
    PortListError(String),
    // Messages for plotting
    DataReceived(f64),
    // Messages for errors
    ErrorOccurred(String),
}

// --- Chart Data Structure ---
#[derive(Debug, Clone)]
struct LineChart {
    data: Vec<(f64, f64)>,
}

impl LineChart {
    fn new(data: Vec<(f64, f64)>) -> Self {
        LineChart { data }
    }
}

// --- `iced::Application` implementation ---
impl iced::Application for ComApp {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = ();
    type Theme = Theme;

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                baud_rate: BAUD_DEFAULT,
                ports_names: vec![],
                serial_port_name: None,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                ..Default::default()
            },
            Command::perform(list_serial_ports(), |res| match res {
                Ok(ports) => Message::PortListReceived(ports),
                Err(e) => Message::PortListError(e.to_string()),
            }),
        )
    }

    fn title(&self) -> String {
        String::from("Serial Terminal")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Connect => {
                if let Some(port_name) = self.serial_port_name.clone() {
                    let baud_rate = self.baud_rate;
                    let (tx, rx) = mpsc::channel(1);
                    self.tx = Some(tx);
                    let rx_handle = tokio::spawn(handle_serial_read(
                        port_name.clone(),
                        baud_rate,
                        self.tx.clone().unwrap(),
                    ));
                    self.is_connected = true;
                    self.rx_task = Some(rx_handle);
                    return Command::none();
                }
            }
            Message::Disconnect => {
                if let Some(handle) = self.rx_task.take() {
                    handle.abort();
                }
                self.is_connected = false;
                self.tx = None;
                return Command::none();
            }
            Message::PortSelected(port) => {
                self.serial_port_name = Some(port);
                return Command::none();
            }
            Message::BaudRateSelected(baud) => {
                self.baud_rate = baud;
                return Command::none();
            }
            Message::BaudRateTextChanged(text) => {
                if let Ok(baud) = text.parse::<u32>() {
                    self.baud_rate = baud;
                }
                return Command::none();
            }
            Message::InputTextChanged(text) => {
                self.input_text = text;
                return Command::none();
            }
            Message::Send => {
                // Not implemented yet
                self.input_text.clear();
                return Command::none();
            }
            Message::SerialDataReceived(data) => {
                if let Ok(s) = String::from_utf8(data) {
                    self.buffer.push_str(&s);
                }
                return Command::none();
            }
            Message::ListPorts => {
                return Command::perform(list_serial_ports(), |res| match res {
                    Ok(ports) => Message::PortListReceived(ports),
                    Err(e) => Message::PortListError(e.to_string()),
                });
            }
            Message::PortListReceived(ports) => {
                self.ports_names = ports;
                if let Some(port) = self.ports_names.first() {
                    self.serial_port_name = Some(port.clone());
                }
                return Command::none();
            }
            Message::PortListError(e) => {
                self.errors.push(e);
                return Command::none();
            }
            Message::DataReceived(value) => {
                self.data_points.push((self.data_counter as f64, value));
                self.data_counter += 1;
                if self.data_points.len() > MAX_DATA_POINTS {
                    self.data_points.remove(0);
                }
                return Command::none();
            }
            Message::ErrorOccurred(e) => {
                self.errors.push(e);
                return Command::none();
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let port_list_selector = iced::widget::pick_list(
            self.ports_names.clone(),
            self.serial_port_name.clone(),
            Message::PortSelected,
        );

        let baud_rate_selector = iced::widget::pick_list(
            Vec::from_iter(BAUD_RATES.iter().copied()),
            Some(self.baud_rate),
            Message::BaudRateSelected,
        );

        let port_settings = row![
            text("Port:"),
            port_list_selector,
            horizontal_space(Length::Fill),
            text("Baud:"),
            baud_rate_selector,
            horizontal_space(Length::Fill),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let connect_button = if self.is_connected {
            button("Disconnect").on_press(Message::Disconnect)
        } else {
            button("Connect").on_press(Message::Connect)
        };

        let serial_input = row![
            text_input("Send command...", &self.input_text)
                .on_input(Message::InputTextChanged)
                .on_submit(Message::Send),
            button("Send").on_press(Message::Send),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let buffer_display = text(&self.buffer);

        let chart = ChartWidget::new(LineChart::new(self.data_points.clone()), &());

        let main_content = column![
            port_settings,
            connect_button,
            vertical_space(Length::Units(20)),
            serial_input,
            vertical_space(Length::Units(20)),
            text("Output:"),
            buffer_display,
            vertical_space(Length::Units(20)),
            chart,
        ]
        .spacing(10);

        container(main_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .center_x()
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.is_connected {
            let serial_sub = serial_data_stream(
                self.serial_port_name.clone().unwrap_or_default(),
                self.baud_rate,
            );

            // This is a placeholder subscription for writing, it doesn't do anything yet.
            let tx_sub = if let Some(tx) = self.tx.clone() {
                // In a real app, this subscription would listen for outgoing messages.
                // For now, we'll just return an empty subscription.
                Subscription::none()
            } else {
                Subscription::none()
            };

            Subscription::batch([serial_sub, tx_sub])
        } else {
            Subscription::none()
        }
    }
}

// --- Chart implementation ---
impl Chart<Message> for LineChart {
    type Renderer = iced::Renderer;
    type State = ();

    fn build_chart<DB: DrawingBackend>(
        &self,
        _state: &Self::State,
        root: &mut DrawingArea<DB, Shift>,
    ) -> Result<(), DrawingArea<DB, Shift>> {
        use plotters::style::colors::RED;

        let (max_x, max_y) = self.data.iter().fold((0.0, 0.0), |(max_x, max_y), (x, y)| {
            (x.max(max_x), y.max(max_y))
        });

        let chart_range_x = if self.data.is_empty() {
            0.0..10.0
        } else {
            0.0..(max_x + 10.0)
        };
        let chart_range_y = if self.data.is_empty() {
            0.0..10.0
        } else {
            0.0..(max_y + 10.0)
        };

        let mut chart = ChartBuilder::on(root)
            .caption("Serial Data Plot", ("sans-serif", 50).into_font())
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(40)
            .build_cartesian_2d(chart_range_x, chart_range_y)?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .draw()?;

        chart.draw_series(LineSeries::new(self.data.clone(), &RED))?;

        Ok(())
    }
}

// --- Serial port subscriptions and functions ---

// Asynchronous function to list available serial ports.
async fn list_serial_ports() -> Result<Vec<String>, serialport::Error> {
    let ports = serialport::available_ports()?;
    let port_names = ports.into_iter().map(|p| p.port_name).collect();
    Ok(port_names)
}

// Asynchronous function to handle serial port reads in a separate task.
async fn handle_serial_read(port_name: String, baud_rate: u32, tx: mpsc::Sender<String>) -> () {
    let mut serial_port = match tokio_serial::new(port_name, baud_rate)
        .timeout(Duration::from_millis(100))
        .open_native_async()
    {
        Ok(port) => port,
        Err(e) => {
            let _ = tx.send(format!("Error opening serial port: {}", e)).await;
            return;
        }
    };

    let mut buf = vec![0u8; 1024];
    loop {
        match serial_port.read(&mut buf).await {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    if let Ok(s) = String::from_utf8(buf[..bytes_read].to_vec()) {
                        let _ = tx.send(s).await;
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
            Err(e) => {
                let _ = tx
                    .send(format!("Error reading from serial port: {}", e))
                    .await;
                break;
            }
        }
    }
}

// Subscription to read data from the serial port.
fn serial_data_stream(port_name: String, baud_rate: u32) -> Subscription<Message> {
    struct SerialStream;

    iced::Subscription::run(iced::futures::stream::unfold(
        (Some(port_name), Some(baud_rate)),
        move |state| {
            let (port_name, baud_rate) = state;
            async move {
                if let (Some(port), Some(baud)) = (port_name.clone(), baud_rate) {
                    let mut serial = tokio_serial::new(port, baud)
                        .data_bits(DataBits::Eight)
                        .parity(Parity::None)
                        .stop_bits(StopBits::One)
                        .timeout(Duration::from_millis(100))
                        .open_native_async()
                        .ok();

                    if let Some(mut serial_port) = serial {
                        let mut buf = vec![0u8; 1024];
                        loop {
                            match serial_port.read(&mut buf).await {
                                Ok(bytes_read) if bytes_read > 0 => {
                                    return Some((
                                        Message::SerialDataReceived(buf[..bytes_read].to_vec()),
                                        (Some(port_name), Some(baud)),
                                    ));
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    return Some((
                                        Message::ErrorOccurred(e.to_string()),
                                        (None, None),
                                    ));
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    } else {
                        return Some((
                            Message::ErrorOccurred("Could not open serial port".to_string()),
                            (None, None),
                        ));
                    }
                }
                None
            }
        },
    ))
}

// Main function to run the application.
fn main() -> iced::Result {
    ComApp::run(Settings::default())
}
