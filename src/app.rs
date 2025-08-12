use iced::{Element, Subscription, Task};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_serial::SerialStream;

pub use crate::ui::*; // re-export UI types if needed

pub struct App {
    // basic state
    pub terminal: String,
    pub input: String,

    // serial handle (optional)
    pub port: Option<Arc<Mutex<SerialStream>>>,

    // UI settings (placeholders)
    pub selected_port: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    // UI messages
    NoOp,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                terminal: String::new(),
                input: String::new(),
                port: None,
                selected_port: None,
            },
            Task::none(),
        )
    }

    pub fn update(&mut self, _message: Message) -> Task<Message> {
        // TODO: implement update logic
        Task::none()
    }

    pub fn view(&self) -> Element<Message> {
        // delegate to ui module
        crate::ui::view(self)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // TODO: combine serial subscription + periodic tasks
        Subscription::none()
    }
}
