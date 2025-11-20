use serialport::SerialPort;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;

fn main() {
    // Виклик: cargo run -- COM5 aaa.wav
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: send_wav <COM port> <file.wav>");
        return;
    }

    let port_name = &args[1];
    let filename = &args[2];

    // Відкриваємо COM‑порт
    let mut port = serialport::new(port_name, 115200)
        .timeout(Duration::from_secs(1))
        .open()
        .expect("Failed to open port");

    // Команда start
    let start_cmd = format!("start {}\n", filename);
    port.write_all(start_cmd.as_bytes()).unwrap();

    // Відправка файла блоками
    let mut f = File::open(filename).expect("Failed to open file");
    let mut buf = [0u8; 512];
    loop {
        let n = f.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        port.write_all(&buf[..n]).unwrap();
        std::thread::sleep(Duration::from_millis(10)); // невелика пауза
    }

    // Команда stop
    port.write_all(b"stop\n").unwrap();

    println!("File {} sent successfully!", filename);
}
