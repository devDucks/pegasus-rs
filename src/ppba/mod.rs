use hex::FromHex;
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use std::io::{Read, Write};
use std::time::Duration;
use uuid::Uuid;

use log::{debug, error, info};

enum Command {
    /// Adjustable 12V Output SET command is P2:
    Adj12VOutput = 0x50323a,
    /// DewA power SET command is P3:
    Dew1Power = 0x50333a,
    /// DewB power SET command is P4:
    Dew2Power = 0x50343a,
    /// Status command serial code is P#
    Status = 0x5023,
    /// Firmware version command serial code is PV
    FirmwareVersion = 0x5056,
    /// Power consumption and stats serial code is PS
    PowerConsumAndStats = 0x5053,
    /// Power metrics serial code is PC
    PowerMetrics = 0x5043,
    /// Power and sensor reading serial code is PA
    PowerAndSensorReadings = 0x5041,
    /// Power status on boot SET command is PE:
    PowerStatusOnBoot = 0x50453a,
    /// Quad port boot status SET command is P1:
    QuadPortStatus = 0x50313a,
    /// Reboot command is PF
    Reboot = 0x5046,
}

pub struct Property {
    pub name: String,
    pub value: String,
    pub kind: String,
    pub permission: Permission,
}

pub struct BaseDevice {
    pub id: Uuid,
    pub name: String,
    pub properties: Vec<Property>,
    pub address: String,
    pub baud: u32,
    #[cfg(unix)]
    port: TTYPort,
    #[cfg(windows)]
    port: COMPort,
}

#[derive(Copy, Clone, Debug)]
pub enum Permission {
    ReadOnly = 0,
    WriteOnly = 1,
    ReadWrite = 2,
}

pub type PowerBoxDevice = BaseDevice;

#[derive(Debug)]
pub enum DeviceError {
    CannotConnect,
    ComError,
    Timeout,
    CannotUpdateReadOnlyProperty,
    UnknownProperty,
    InvalidValue,
}

impl BaseDevice {
    pub fn new(name: &str, address: &str, baud: u32) -> Result<Self, DeviceError> {
        let builder = serialport::new(address, baud).timeout(Duration::from_millis(500));

        if let Ok(port_) = builder.open_native() {
            let mut dev = Self {
                id: Uuid::new_v4(),
                name: name.to_owned(),
                properties: Vec::new(),
                address: address.to_owned(),
                baud: baud,
                port: port_,
            };
            match dev.send_command(Command::Status, None) {
                Ok(_) => {
                    dev.init_props();
                    Ok(dev)
                }
                Err(_) => Err(DeviceError::CannotConnect),
            }
        } else {
            Err(DeviceError::CannotConnect)
        }
    }
}
const POWER_STATS: [(&str, &str, Permission); 4] = [
    ("average_amps", "float", Permission::ReadOnly),
    ("amps_hours", "float", Permission::ReadOnly),
    ("watt_hours", "float", Permission::ReadOnly),
    ("uptime", "integer", Permission::ReadOnly),
];

const POWER_METRICS: [(&str, &str, Permission); 4] = [
    ("total_current", "float", Permission::ReadOnly),
    ("current_12V_output", "float", Permission::ReadOnly),
    ("current_dewA", "float", Permission::ReadOnly),
    ("current_dewB", "float", Permission::ReadOnly),
];

const POWER_SENSOR_READINGS: [(&str, &str, Permission); 12] = [
    ("input_voltage", "float", Permission::ReadOnly),
    ("current", "float", Permission::ReadOnly),
    ("temp", "float", Permission::ReadOnly),
    ("humidity", "float", Permission::ReadOnly),
    ("dew_point", "float", Permission::ReadOnly),
    ("quadport_status", "boolean", Permission::ReadWrite),
    ("adj_output_status", "boolean", Permission::ReadOnly),
    ("dew1_power", "integer", Permission::ReadWrite),
    ("dew2_power", "integer", Permission::ReadWrite),
    ("autodew_bool", "boolean", Permission::ReadOnly),
    ("pwr_warn", "boolean", Permission::ReadOnly),
    ("adjustable_output", "integer", Permission::ReadWrite),
];

const WRITE_ONLY_PROPERTIES: [(&str, &str, &str, Permission); 2] = [
    ("reboot", "bool", "0", Permission::WriteOnly),
    (
        "power_status_on_boot",
        "string",
        "1111",
        Permission::WriteOnly,
    ),
];

trait Pegasus {
    fn send_command(&mut self, comm: Command, val: Option<&str>) -> Result<String, DeviceError>;
    fn firmware_version(&mut self) -> Property;
    fn power_consumption_and_stats(&mut self) -> Vec<Property>;
    fn power_metrics(&mut self) -> Vec<Property>;
    fn power_and_sensor_readings(&mut self) -> Vec<Property>;
    fn create_write_only_properties(&mut self) -> Vec<Property>;
}

pub trait AstronomicalDevice {
    fn init_props(&mut self);
    fn get_properties(&self) -> &Vec<Property>;
    fn update_property(&mut self, prop_name: &str, val: &str) -> Result<(), DeviceError>;
    fn update_property_remote(&mut self, prop_name: &str, val: &str) -> Result<(), DeviceError>;
    fn find_property_index(&self, prop_name: &str) -> Option<usize>;
}

impl AstronomicalDevice for PowerBoxDevice {
    fn init_props(&mut self) {
        let fw = self.firmware_version();
        let wo_props = self.create_write_only_properties();

        for prop in self.power_consumption_and_stats() {
            self.properties.push(prop);
        }
        for prop in self.power_metrics() {
            self.properties.push(prop);
        }
        for prop in self.power_and_sensor_readings() {
            self.properties.push(prop);
        }
        for prop in wo_props {
            self.properties.push(prop);
        }
        self.properties.push(fw);
    }

    fn get_properties(&self) -> &Vec<Property> {
        &self.properties
    }

    fn find_property_index(&self, prop_name: &str) -> Option<usize> {
        let mut index = 256;

        for (idx, prop) in self.properties.iter().enumerate() {
            if prop.name == prop_name {
                index = idx;
                break;
            }
        }
        if index == 256 {
            None
        } else {
            Some(index)
        }
    }

    /// Updates the local value of a given property in the state machine
    fn update_property(&mut self, prop_name: &str, val: &str) -> Result<(), DeviceError> {
        if let Some(prop_idx) = self.find_property_index(prop_name) {
            let r_prop = self.properties.get(prop_idx).unwrap();
            match r_prop.permission {
                Permission::ReadOnly => Err(DeviceError::CannotUpdateReadOnlyProperty),
                _ => match self.update_property_remote(prop_name, val) {
                    Ok(_) => {
                        let prop = self.properties.get_mut(prop_idx).unwrap();
                        // Adjustable output is a special one, it can set both status AND power,
                        // but 0 and 1 actually change adjustable_output_status
                        if prop.name == "adjustable_output" && (val == "0" || val == "1") {
                            return Ok(());
                        }
                        prop.value = val.to_owned();
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                },
            }
        } else {
            Err(DeviceError::UnknownProperty)
        }
    }

    /// Updates the value of the device on the device itself
    fn update_property_remote(&mut self, prop_name: &str, val: &str) -> Result<(), DeviceError> {
        match prop_name {
            "adjustable_output" => {
                self.send_command(Command::Adj12VOutput, Some(val))?;
                Ok(())
            }
            "quadport_status" => {
                self.send_command(Command::QuadPortStatus, Some(val))?;
                Ok(())
            }
            "dew1_power" => {
                self.send_command(Command::Dew1Power, Some(val))?;
                Ok(())
            }
            "dew2_power" => {
                self.send_command(Command::Dew2Power, Some(val))?;
                Ok(())
            }
            "power_status_on_boot" => {
                self.send_command(Command::PowerStatusOnBoot, Some(val))?;
                Ok(())
            }
            "reboot" => {
                self.send_command(Command::Reboot, None)?;
                Ok(())
            }
            _ => Err(DeviceError::UnknownProperty),
        }
    }
}

impl Pegasus for PowerBoxDevice {
    fn send_command(&mut self, comm: Command, val: Option<&str>) -> Result<String, DeviceError> {
        // First convert the command into an hex STRING
        let mut hex_command = format!("{:X}", comm as i32);

        if let Some(value) = val {
            hex_command += hex::encode(value).as_str();
        }

        // Cast the hex string to a sequence of bytes
        let mut command: Vec<u8> = Vec::from_hex(hex_command).expect("Invalid Hex String");
        // append \n at the end
        command.push(10);

        match self.port.write(&command) {
            Ok(_) => {
                info!(
                    "Sent command: {}",
                    std::str::from_utf8(&command[..command.len() - 1]).unwrap()
                );
                let mut final_buf: Vec<u8> = Vec::new();
                debug!("Receiving data");

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
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                            return Err(DeviceError::Timeout)
                        }
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
                // Strip the carriage return from the response
                let response = std::str::from_utf8(&final_buf[..&final_buf.len() - 2]).unwrap();
                debug!("RESPONSE: {}", response);
                let resp: Vec<&str> = response.split(":").collect();

                if resp.len() > 1 && resp[1] == "ERR" {
                    Err(DeviceError::InvalidValue)
                } else {
                    Ok(response.to_owned())
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Err(DeviceError::Timeout),
            Err(e) => {
                error!("{:?}", e);
                Err(DeviceError::ComError)
            }
        }
    }

    fn firmware_version(&mut self) -> Property {
        if let Ok(fw) = self.send_command(Command::FirmwareVersion, None) {
            Property {
                name: "firmware_version".to_owned(),
                value: fw,
                kind: "string".to_owned(),
                permission: Permission::ReadOnly,
            }
        } else {
            Property {
                name: "firmware_version".to_owned(),
                value: "UNKNOWN".to_owned(),
                kind: "string".to_owned(),
                permission: Permission::ReadOnly,
            }
        }
    }

    fn power_consumption_and_stats(&mut self) -> Vec<Property> {
        if let Ok(stats) = self.send_command(Command::PowerConsumAndStats, None) {
            debug!("POWER CONSUMPTIONS STATS: {}", stats);
            let chunks: Vec<&str> = stats.split(":").collect();
            let slice = &chunks.as_slice()[1..];
            let mut props = Vec::new();

            for (index, chunk) in slice.iter().enumerate() {
                props.push(Property {
                    name: POWER_STATS[index].0.to_string(),
                    value: chunk.to_string(),
                    kind: POWER_STATS[index].1.to_string(),
                    permission: Permission::ReadOnly,
                })
            }
            props
        } else {
            vec![]
        }
    }

    fn power_metrics(&mut self) -> Vec<Property> {
        if let Ok(stats) = self.send_command(Command::PowerMetrics, None) {
            debug!("POWER METRICS STATS:{}", stats);
            let chunks: Vec<&str> = stats.split(":").collect();
            let slice = &chunks.as_slice()[1..chunks.len() - 1];
            let mut props = Vec::new();

            for (index, chunk) in slice.iter().enumerate() {
                props.push(Property {
                    name: POWER_METRICS[index].0.to_string(),
                    value: chunk.to_string(),
                    kind: POWER_METRICS[index].1.to_string(),
                    permission: Permission::ReadOnly,
                })
            }
            props
        } else {
            vec![]
        }
    }

    fn power_and_sensor_readings(&mut self) -> Vec<Property> {
        if let Ok(stats) = self.send_command(Command::PowerAndSensorReadings, None) {
            debug!("POWER AND SENSORS READINGS: {}", stats);
            let chunks: Vec<&str> = stats.split(":").collect();
            let slice = &chunks.as_slice()[1..];
            let mut props = Vec::new();
            for (index, chunk) in slice.iter().enumerate() {
                props.push(Property {
                    name: POWER_SENSOR_READINGS[index].0.to_string(),
                    value: chunk.to_string(),
                    kind: POWER_SENSOR_READINGS[index].1.to_string(),
                    permission: POWER_SENSOR_READINGS[index].2,
                })
            }
            props
        } else {
            vec![]
        }
    }

    fn create_write_only_properties(&mut self) -> Vec<Property> {
        let mut props = Vec::with_capacity(WRITE_ONLY_PROPERTIES.len());

        for (name, kind, value, perm) in WRITE_ONLY_PROPERTIES {
            props.push(Property {
                name: name.to_string(),
                value: value.to_string(),
                kind: kind.to_string(),
                permission: perm,
            });
        }
        props
    }
}
