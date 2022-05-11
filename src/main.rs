use std::io::Write;
use std::time::Duration;

mod utils {
    use serialport::{available_ports, SerialPortType, UsbPortInfo};

    pub fn look_for_devices(device_name: &str) -> Vec<(String, UsbPortInfo)> {
        let ports = available_ports().unwrap();
        let mut devices = Vec::new();

        for port in ports {
            if let SerialPortType::UsbPort(info) = port.port_type {
                if let Some(ref serial) = info.serial_number {
                    if &serial[0..4] == device_name {
                        devices.push((port.port_name, info));
                    }
                }
            }
        }
        devices
    }
}
fn main() {
    let devices = utils::look_for_devices("PPBA");
    let command = "P#\n";

    if devices.is_empty() {
        println!("empty");
    } else {
        let rate = 0;
        let builder = serialport::new(&devices[0].0, 9600);
        let mut port = builder.open().unwrap();

        loop {
            match port.write(&command.as_bytes()) {
                Ok(_) => {
                    println!("Sent: {}", &command);
                    std::io::stdout().flush().unwrap();
                    let mut final_buf: Vec<u8> = Vec::new();
                    println!("Receiving data");

                    loop {
                        let mut read_buf = [0xAA; 1];

                        match port.read(read_buf.as_mut_slice()) {
                            Ok(t) => {
                                println!("We have {} bytes to read", t);
                                println!("We read: {:?}", &read_buf);
                                let byte = read_buf[0];
                                if byte == '\n' as u8 {
                                    break;
                                }
                                final_buf.push(byte);

                                println!("Actual final buffer: {:?}", &final_buf);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                            Err(e) => eprintln!("{:?}", e),
                        }
                    }
                    println!(
                        "We read: {:?} which as string is {}",
                        &final_buf,
                        std::str::from_utf8(&final_buf).unwrap()
                    );
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                Err(e) => eprintln!("{:?}", e),
            }
            if rate == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis((1000.0 / (rate as f32)) as u64));
        }
    }
}
