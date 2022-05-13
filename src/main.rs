use std::time::Duration;

use pegasus_rs::ppba::{AstronomicalDevice, PowerBoxDevice};

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
    let found = utils::look_for_devices("PPBA");
    let mut devices: Vec<PowerBoxDevice> = Vec::new();

    if found.is_empty() {
        println!("empty");
    } else {
        for dev in found {
            let mut device_name = String::from("PegausPowerBoxAdvanced");
            println!("name: {}", dev.0);
            println!("info: {:?}", dev.1);

            if let Some(serial) = dev.1.serial_number {
                device_name = device_name + &serial
            }
            if let Ok(device) = PowerBoxDevice::new(&device_name, &dev.0, 9600) {
                devices.push(device)
            } else {
                println!("Cannot start communication with {}", &device_name);
            }
        }
    }

    for mut d in devices {
        let props = d.get_properties();
        for prop in &props {
            println!(
                "name: {} | value: {} | kind: {} | read_only: {}",
                prop.name, prop.value, prop.kind, prop.read_only
            );
        }
    }
    std::thread::sleep(Duration::from_millis(5000));
}
