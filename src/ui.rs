use crate::app::App;
use crate::app::Message;
use iced::{
    widget::{column, text},
    Element,
};

pub fn view(app: &App) -> Element<Message> {
    // minimal placeholder view (replace with full UI in next steps)
    column![
        text("Iced COM terminal - scaffold"),
        text("Terminal will appear here..."),
    ]
    .into()
}
