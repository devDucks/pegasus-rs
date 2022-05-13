use hex::FromHex;
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use std::io::{Read, Write};

enum Command {
    /// Status command is P#
    Status = 0x5023,
    /// Firmware version command is PV
    FirmwareVersion = 0x5056,
    /// Power consumption and stats is PS
    PowerConsumAndStats = 0x5053,
    /// Power metrics is PC
    PowerMetrics = 0x5043,
}

pub struct Property {
    pub name: String,
    pub value: String,
    pub kind: String,
    pub read_only: bool,
}

pub struct BaseDevice {
    pub name: String,
    pub properties: Vec<Property>,
    pub address: String,
    pub baud: u32,
    #[cfg(unix)]
    port: TTYPort,
    #[cfg(windows)]
    port: COMPort,
}

pub type PowerBoxDevice = BaseDevice;

pub enum DeviceError {
    CannotConnect,
    ComError,
    Timeout,
}

impl BaseDevice {
    pub fn new(name: &str, address: &str, baud: u32) -> Result<Self, DeviceError> {
        let builder = serialport::new(address, baud);

        if let Ok(port_) = builder.open_native() {
            let mut dev = Self {
                name: name.to_owned(),
                properties: Vec::new(),
                address: address.to_owned(),
                baud: baud,
                port: port_,
            };
            match dev.send_command(Command::Status) {
                Ok(_) => Ok(dev),
                Err(_) => Err(DeviceError::CannotConnect),
            }
        } else {
            Err(DeviceError::CannotConnect)
        }
    }
}
const POWER_STATS: [(&str, &str); 4] = [
    ("average_amps", "float"),
    ("amps_hours", "float"),
    ("watt_hours", "float"),
    ("uptime", "integer"),
];

const POWER_METRICS: [(&str, &str); 4] = [
    ("total_current", "float"),
    ("current_12V_output", "float"),
    ("current_dewA", "float"),
    ("current_dewB", "float"),
];

trait Pegasus {
    fn send_command(&mut self, comm: Command) -> Result<String, DeviceError>;
    fn firmware_version(&mut self) -> Property;
    fn power_consumption_and_stats(&mut self) -> Vec<Property>;
    fn power_metrics(&mut self) -> Vec<Property>;
}

pub trait AstronomicalDevice {
    fn get_properties(&mut self) -> Vec<Property>;
}

impl AstronomicalDevice for PowerBoxDevice {
    fn get_properties(&mut self) -> Vec<Property> {
        let mut properties = Vec::new();
        let fw = self.firmware_version();
        for prop in self.power_consumption_and_stats() {
            properties.push(prop);
        }
        for prop in self.power_metrics() {
            properties.push(prop);
        }
        properties.push(fw);
        properties
    }
}

impl Pegasus for PowerBoxDevice {
    fn send_command(&mut self, comm: Command) -> Result<String, DeviceError> {
        // First convert the command into an hex STRING
        let hex_command = format!("{:X}", comm as i32);
        // Cast the hex string to a sequence of bytes
        let mut command: Vec<u8> = Vec::from_hex(hex_command).expect("Invalid Hex String");
        // append \n at the end
        command.push(10);

        match self.port.write(&command) {
            Ok(_) => {
                println!("Sent command: {}", std::str::from_utf8(&command).unwrap());

                let mut final_buf: Vec<u8> = Vec::new();
                println!("Receiving data");

                loop {
                    let mut read_buf = [0xAA; 1];

                    match self.port.read(read_buf.as_mut_slice()) {
                        Ok(_) => {
                            let byte = read_buf[0];

                            final_buf.push(byte);

                            if byte == '\n' as u8 {
                                break;
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
                // Strip the carriage return from the response
                let response = std::str::from_utf8(&final_buf[..&final_buf.len() - 2]).unwrap();
                println!("RESPONSE: {}", response);
                Ok(response.to_owned())
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Err(DeviceError::Timeout),
            Err(e) => {
                println!("{:?}", e);
                Err(DeviceError::ComError)
            }
        }
    }

    fn firmware_version(&mut self) -> Property {
        if let Ok(fw) = self.send_command(Command::FirmwareVersion) {
            Property {
                name: "firmware_version".to_owned(),
                value: fw,
                kind: "string".to_owned(),
                read_only: true,
            }
        } else {
            Property {
                name: "firmware_version".to_owned(),
                value: "UNKNOWN".to_owned(),
                kind: "string".to_owned(),
                read_only: true,
            }
        }
    }

    fn power_consumption_and_stats(&mut self) -> Vec<Property> {
        if let Ok(stats) = self.send_command(Command::PowerConsumAndStats) {
            println!("{}", stats);
            let chunks: Vec<&str> = stats.split(":").collect();
            let slice = &chunks.as_slice()[1..];
            let mut props = Vec::new();
            println!("{:?}", chunks);
            for (index, chunk) in slice.iter().enumerate() {
                props.push(Property {
                    name: POWER_STATS[index].0.to_string(),
                    value: chunk.to_string(),
                    kind: POWER_STATS[index].1.to_string(),
                    read_only: true,
                })
            }
            props
        } else {
            vec![]
        }
    }

    fn power_metrics(&mut self) -> Vec<Property> {
        if let Ok(stats) = self.send_command(Command::PowerMetrics) {
            println!("{}", stats);
            let chunks: Vec<&str> = stats.split(":").collect();
            let slice = &chunks.as_slice()[1..chunks.len() - 1];
            let mut props = Vec::new();

            for (index, chunk) in slice.iter().enumerate() {
                println!("LINE: {} | CHUNK: {}", index, chunk);
                props.push(Property {
                    name: POWER_METRICS[index].0.to_string(),
                    value: chunk.to_string(),
                    kind: POWER_METRICS[index].1.to_string(),
                    read_only: true,
                })
            }
            props
        } else {
            vec![]
        }
    }
}
