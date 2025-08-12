use serialport::SerialPortInfo;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_serial::{SerialPortBuilderExt, SerialStream};

pub async fn list_ports() -> Vec<String> {
    match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
        Err(_) => vec![],
    }
}

pub async fn open_port_async(
    port_name: &str,
    baud: u32,
) -> Result<Arc<Mutex<SerialStream>>, String> {
    match tokio_serial::new(port_name, baud).open_native_async() {
        Ok(s) => Ok(Arc::new(Mutex::new(s))),
        Err(e) => Err(e.to_string()),
    }
}
