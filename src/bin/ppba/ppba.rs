use astrotools::device::{DeviceType, LightspeedDevice};
use astrotools::properties::{
    Permission, Prop, PropValue, Property, PropertyErrorType, UpdatePropertyRequest,
};
use astrotools::LightspeedError;
use hex::FromHex;
use log::{debug, error, info};
use serde::Serialize;
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use std::fmt::UpperHex;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;
use uuid::Uuid;

enum PpbaCommand {
    Update(UpdatePropertyRequest),
}

#[derive(Debug, Serialize)]
pub struct PegasusPowerBox {
    #[serde(skip)]
    id: Uuid,
    name: String,
    address: String,
    pub baud: u32,
    #[cfg(unix)]
    #[serde(skip)]
    pub port: TTYPort,
    #[cfg(windows)]
    #[serde(skip)]
    pub port: COMPort,
    fw_version: Property<String>,
    reboot: Property<bool>,
    input_voltage: Property<f32>,
    current: Property<f32>,
    temperature: Property<f32>,
    humidity: Property<f32>,
    quadport_status: Property<bool>,
    adj_output_status: Property<bool>,
    dew1_power: Property<u8>,
    dew1_current: Property<f32>,
    dew2_power: Property<u8>,
    dew2_current: Property<f32>,
    autodew: Property<bool>,
    pwr_warn: Property<bool>,
    adj_output: Property<u8>,
    average_amps: Property<f32>,
    amps_hours: Property<f32>,
    watt_hours: Property<f32>,
    uptime: Property<u32>,
    total_current: Property<f32>,
    current_12v_output: Property<f32>,
    #[serde(skip)]
    cmd_tx: SyncSender<PpbaCommand>,
    #[serde(skip)]
    cmd_rx: Receiver<PpbaCommand>,
}

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

trait Pegasus {
    fn update_firmware_version(&mut self);
    fn update_power_consumption_and_stats(&mut self);
    fn update_power_metrics(&mut self);
    fn update_power_and_sensor_readings(&mut self);
}

impl PegasusPowerBox {
    pub fn new(name: &str, address: &str, baud: u32, timeout_ms: u64) -> Result<Self, String> {
        let port_ = serialport::new(address, baud)
            .timeout(Duration::from_millis(timeout_ms))
            .open_native()
            .map_err(|e| format!("Cannot open port {address}: {e}"))?;

        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<PpbaCommand>(32);

        let mut dev = Self {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            address: address.to_owned(),
            baud,
            port: port_,
            fw_version: Property::<String>::new("UNKNOWN".to_string(), Permission::ReadOnly),
            reboot: Property::<bool>::new(false, Permission::ReadWrite),
            input_voltage: Property::<f32>::new(0.0, Permission::ReadOnly),
            current: Property::<f32>::new(0.0, Permission::ReadOnly),
            temperature: Property::<f32>::new(0.0, Permission::ReadOnly),
            humidity: Property::<f32>::new(0.0, Permission::ReadOnly),
            quadport_status: Property::<bool>::new(false, Permission::ReadWrite),
            adj_output: Property::<u8>::new(0, Permission::ReadWrite),
            adj_output_status: Property::<bool>::new(false, Permission::ReadWrite),
            dew1_power: Property::<u8>::new(0, Permission::ReadWrite),
            dew1_current: Property::<f32>::new(0.0, Permission::ReadOnly),
            dew2_power: Property::<u8>::new(0, Permission::ReadWrite),
            dew2_current: Property::<f32>::new(0.0, Permission::ReadOnly),
            autodew: Property::<bool>::new(false, Permission::ReadWrite),
            pwr_warn: Property::<bool>::new(false, Permission::ReadOnly),
            average_amps: Property::<f32>::new(0.0, Permission::ReadOnly),
            amps_hours: Property::<f32>::new(0.0, Permission::ReadOnly),
            watt_hours: Property::<f32>::new(0.0, Permission::ReadOnly),
            uptime: Property::<u32>::new(0, Permission::ReadOnly),
            total_current: Property::<f32>::new(0.0, Permission::ReadOnly),
            current_12v_output: Property::<f32>::new(0.0, Permission::ReadOnly),
            cmd_tx,
            cmd_rx,
        };

        dev.send_command(Command::Status as i32, None)
            .map_err(|e| format!("Status command failed: {e}"))?;
        dev.update_firmware_version();
        dev.fetch_props();
        Ok(dev)
    }

    fn send_command<T>(&mut self, comm: T, val: Option<String>) -> Result<String, String>
    where
        T: UpperHex,
    {
        let mut hex_command = format!("{:X}", comm);

        if let Some(value) = val {
            hex_command += hex::encode(value).as_str();
        }

        let mut command: Vec<u8> = Vec::from_hex(hex_command).expect("Invalid Hex String");
        command.push(10);

        match self.port.write(&command) {
            Ok(_) => {
                debug!(
                    "Sent command: {}",
                    std::str::from_utf8(&command[..command.len() - 1]).unwrap()
                );
                let mut final_buf: Vec<u8> = Vec::new();

                loop {
                    let mut read_buf = [0xA; 1];
                    match self.port.read(read_buf.as_mut_slice()) {
                        Ok(_) => {
                            let byte = read_buf[0];
                            final_buf.push(byte);
                            if byte == b'\n' {
                                break;
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                            return Err("Timeout".to_string())
                        }
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
                let response = std::str::from_utf8(&final_buf[..&final_buf.len() - 2]).unwrap();
                debug!("RESPONSE: {}", response);
                let resp: Vec<&str> = response.split(':').collect();
                if resp.len() > 1 && resp[1] == "ERR" {
                    Err("Invalid value".to_string())
                } else {
                    Ok(response.to_owned())
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Err("Timeout".to_string()),
            Err(e) => {
                error!("{:?}", e);
                Err("Communication error".to_string())
            }
        }
    }

    pub fn fetch_props(&mut self) {
        info!("Fetching properties for device {}", self.name);
        self.update_power_consumption_and_stats();
        self.update_power_metrics();
        self.update_power_and_sensor_readings();
    }

    fn process_command(&mut self, cmd: PpbaCommand) {
        match cmd {
            PpbaCommand::Update(req) => {
                if let Err(e) = self.handle_update(&req.prop_name, req.value) {
                    error!("update '{}' failed: {:?}", req.prop_name, e);
                }
            }
        }
    }

    fn handle_update(&mut self, prop_name: &str, val: PropValue) -> Result<(), LightspeedError> {
        match prop_name {
            "dew1_power" => {
                let v = u32::try_from(val)? as u8;
                self.send_command(Command::Dew1Power as i32, Some(v.to_string()))
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.dew1_power.update_int(v);
                Ok(())
            }
            "dew2_power" => {
                let v = u32::try_from(val)? as u8;
                self.send_command(Command::Dew2Power as i32, Some(v.to_string()))
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.dew2_power.update_int(v);
                Ok(())
            }
            "adj_output" => {
                let v = u32::try_from(val)? as u8;
                self.send_command(Command::Adj12VOutput as i32, Some(v.to_string()))
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.adj_output.update_int(v);
                Ok(())
            }
            "quadport_status" => {
                let v = match val {
                    PropValue::Bool(b) => b,
                    PropValue::Int(i) => i != 0,
                    _ => {
                        return Err(LightspeedError::PropertyError(
                            PropertyErrorType::InvalidValue,
                        ))
                    }
                };
                let arg = if v { "1".to_string() } else { "0".to_string() };
                self.send_command(Command::QuadPortStatus as i32, Some(arg))
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.quadport_status.update_int(v);
                Ok(())
            }
            "reboot" => {
                self.send_command(Command::Reboot as i32, None)
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                Ok(())
            }
            _ => Err(LightspeedError::PropertyError(
                PropertyErrorType::InvalidValue,
            )),
        }
    }
}

impl Pegasus for PegasusPowerBox {
    fn update_firmware_version(&mut self) {
        if let Ok(fw) = self.send_command(Command::FirmwareVersion as i32, None) {
            let _ = self.fw_version.update_int(fw);
        }
    }

    fn update_power_consumption_and_stats(&mut self) {
        if let Ok(stats) = self.send_command(Command::PowerConsumAndStats as i32, None) {
            debug!("POWER CONSUMPTIONS STATS: {}", stats);
            let chunks: Vec<&str> = stats.split(':').collect();
            let slice = chunks.as_slice();
            // Response: PS:averageAmps:ampHours:wattHours:uptime_in_milliseconds
            let _ = self.current.update_int(slice[1].parse().unwrap());
            let _ = self.amps_hours.update_int(slice[2].parse().unwrap());
            let _ = self.watt_hours.update_int(slice[3].parse().unwrap());
            let _ = self.uptime.update_int(slice[4].parse().unwrap());
        } else {
            error!("Couldn't read power consumption metrics");
        }
    }

    fn update_power_metrics(&mut self) {
        if let Ok(stats) = self.send_command(Command::PowerMetrics as i32, None) {
            debug!("POWER METRICS STATS:{}", stats);
            let chunks: Vec<&str> = stats.split(':').collect();
            let slice = chunks.as_slice();
            // Response: PC:total_current:current_12V_outputs:current_dewA:current_dewB:uptime
            let _ = self.total_current.update_int(slice[1].parse().unwrap());
            let _ = self
                .current_12v_output
                .update_int(slice[2].parse().unwrap());
            let _ = self.dew1_current.update_int(slice[3].parse().unwrap());
            let _ = self.dew2_current.update_int(slice[4].parse().unwrap());
        } else {
            error!("Couldn't read power metrics stats");
        }
    }

    fn update_power_and_sensor_readings(&mut self) {
        if let Ok(stats) = self.send_command(Command::PowerAndSensorReadings as i32, None) {
            debug!("POWER AND SENSORS READINGS: {}", stats);
            let chunks: Vec<&str> = stats.split(':').collect();
            let slice = chunks.as_slice();
            // Response: PPBA:voltage:current_12V:temp:humidity:dewpoint:quadport:adj_out:dewA:dewB:autodew:pwr_warn:pwradj
            let _ = self.input_voltage.update_int(slice[1].parse().unwrap());
            let _ = self
                .current_12v_output
                .update_int(slice[2].parse().unwrap());
        } else {
            error!("Couldn't read power and sensors reading");
        }
    }
}

impl LightspeedDevice for PegasusPowerBox {
    fn id(&self) -> Uuid {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn dev_type(&self) -> DeviceType {
        DeviceType::PowerBox
    }

    fn command_topics(&self) -> &[&str] {
        &["update"]
    }

    fn state_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    fn dispatcher(&self) -> Box<dyn Fn(&str, &[u8]) -> Result<(), LightspeedError> + Send + Sync> {
        let tx = self.cmd_tx.clone();
        Box::new(move |action, payload| match action {
            "update" => {
                let req: UpdatePropertyRequest =
                    serde_json::from_slice(payload).map_err(|_| LightspeedError::ParseError)?;
                tx.try_send(PpbaCommand::Update(req))
                    .map_err(|_| LightspeedError::QueueFull)
            }
            _ => Err(LightspeedError::UnknownCommand),
        })
    }

    fn tick(&mut self, state_tx: &SyncSender<(Uuid, String)>) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            self.process_command(cmd);
        }
        self.fetch_props();
        state_tx.try_send((self.id, self.state_json())).ok();
    }

    fn close(&mut self) {}
}
