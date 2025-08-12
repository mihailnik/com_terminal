use rfd::FileDialog;

pub fn open_file_blocking() -> Result<String, String> {
    if let Some(p) = FileDialog::new().pick_file() {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

pub fn save_file_blocking(default_name: &str, content: &str) -> Result<(), String> {
    if let Some(p) = FileDialog::new().set_file_name(default_name).save_file() {
        std::fs::write(p, content).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
