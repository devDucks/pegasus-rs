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
