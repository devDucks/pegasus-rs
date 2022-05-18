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
        println!("No Pegasus PPBA found");
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
        println!("ID: {}", d.id);

        for prop in d.get_properties() {
            println!(
                "name: {} | value: {} | kind: {} | read_only: {}",
                prop.name, prop.value, prop.kind, prop.read_only
            );
        }

        match d.update_property("adjustable_output", "0") {
            Ok(_) => println!("Prop adjustable_output updated correctly"),
            Err(e) => println!("Err: {:?}", e),
        }

        match d.update_property("quadport_status", "1") {
            Ok(_) => println!("Prop quadport_status updated correctly"),
            Err(e) => println!("Err: {:?}", e),
        }

        for prop in d.get_properties() {
            println!(
                "name: {} | value: {} | kind: {} | read_only: {}",
                prop.name, prop.value, prop.kind, prop.read_only
            );
        }
    }
}
