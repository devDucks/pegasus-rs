use hex::FromHex;
use log::{debug, error};
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use std::fmt::UpperHex;
use std::io::{Read, Write};
use std::time::Duration;

use crate::PegasusError;

#[allow(dead_code)]
pub enum Command {
    /// P2: — Adjustable 12V output voltage/on-off
    Adj12VOutput = 0x50323a,
    /// P3: — DewA PWM duty cycle [0-255]
    Dew1Power = 0x50333a,
    /// P4: — DewB PWM duty cycle [0-255]
    Dew2Power = 0x50343a,
    /// P# — Device status
    Status = 0x5023,
    /// PV — Firmware version
    FirmwareVersion = 0x5056,
    /// PS — Power consumption statistics
    PowerConsumAndStats = 0x5053,
    /// PC — Power metrics (per-channel currents)
    PowerMetrics = 0x5043,
    /// PA — Power and sensor readings
    PowerAndSensorReadings = 0x5041,
    /// PE: — Set power status on boot
    PowerStatusOnBoot = 0x50453a,
    /// P1: — Quad 12V port on/off
    QuadPortStatus = 0x50313a,
    /// PF — Reboot device
    Reboot = 0x5046,
}

/// Parsed response from the PA command.
///
/// `PPBA:voltage:current_12V:temp:humidity:dewpoint:quadport:adj_out:dewA:dewB:autodew:pwr_warn:pwradj`
pub struct PowerAndSensorReadings {
    pub voltage: f32,
    /// Quad 12V output current in Amps (raw value divided by 65 per spec).
    pub current_12v: f32,
    pub temperature: f32,
    pub humidity: f32,
    pub dewpoint: f32,
    pub quadport_status: bool,
    pub adj_output_status: bool,
    /// DewA PWM duty cycle [0-255].
    pub dew_a_power: u8,
    /// DewB PWM duty cycle [0-255].
    pub dew_b_power: u8,
    pub autodew: bool,
    pub pwr_warn: bool,
    /// Adjustable output selected voltage (3, 5, 7, 8, 9, or 12 V).
    pub adj_output_voltage: u8,
}

/// Parsed response from the PS command.
///
/// `PS:averageAmps:ampHours:wattHours:uptime_ms`
pub struct PowerConsumptionStats {
    pub average_amps: f32,
    pub amp_hours: f32,
    pub watt_hours: f32,
    pub uptime_ms: u32,
}

/// Parsed response from the PC command.
///
/// `PC:total_current:current_12V_outputs:current_dewA:current_dewB:uptime_ms`
pub struct PowerMetrics {
    pub total_current: f32,
    pub current_12v_output: f32,
    pub current_dew_a: f32,
    pub current_dew_b: f32,
    pub uptime_ms: u32,
}

/// Low-level interface for the Pegasus Pocket PowerBox Advanced (PPBA).
///
/// Communicates over a serial port at 9600 8N1.  Each command is terminated
/// with a newline (`\n`); responses are likewise newline-terminated.
#[derive(Debug)]
pub struct Ppba {
    #[cfg(unix)]
    port: TTYPort,
    #[cfg(windows)]
    port: COMPort,
}

impl Ppba {
    /// Open a serial connection to a PPBA and verify it responds to the status
    /// command before returning.
    pub fn new(address: &str, baud: u32, timeout_ms: u64) -> Result<Self, PegasusError> {
        let port = serialport::new(address, baud)
            .timeout(Duration::from_millis(timeout_ms))
            .open_native()
            .map_err(|e| {
                error!("Cannot open port {address}: {e}");
                PegasusError::OpenFailed
            })?;

        let mut ppba = Self { port };
        ppba.send_command(Command::Status as i32, None)?;
        Ok(ppba)
    }

    /// Send a raw command and return the trimmed response line.
    ///
    /// `comm` is encoded as an upper-hex string and written directly to the
    /// port; `val` (if any) is hex-encoded and appended before the newline
    /// terminator.  Returns `Err` if the device replies with `ERR` or a
    /// timeout occurs.
    pub fn send_command<T>(&mut self, cmd: T, val: Option<String>) -> Result<String, PegasusError>
    where
        T: UpperHex,
    {
        let mut hex_cmd = format!("{:X}", cmd);
        if let Some(value) = val {
            hex_cmd += hex::encode(value).as_str();
        }

        let mut command: Vec<u8> = Vec::from_hex(&hex_cmd).map_err(|_| {
            error!("Invalid Hex String: {}", hex_cmd);
            PegasusError::SerialEncode
        })?;
        command.push(10); // newline terminator

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
                            return Err(PegasusError::SerialTimeout);
                        }
                        Err(e) => {
                            error!("Unknown error {:?}", e);
                            return Err(PegasusError::SerialFailure);
                        }
                    }
                }
                // Strip trailing \r\n
                let response =
                    std::str::from_utf8(&final_buf[..final_buf.len() - 2]).map_err(|_| {
                        error!("Unable to convert the response to string");
                        PegasusError::SerialEncode
                    })?;
                debug!("PPBA RESPONSE: {}", response);
                let resp: Vec<&str> = response.split(':').collect();
                if resp.len() > 1 && resp[1] == "ERR" {
                    Err(PegasusError::BadCommand)
                } else {
                    Ok(response.to_owned())
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Err(PegasusError::SerialTimeout)
            }
            Err(e) => {
                error!("Serial error: {}", e);
                Err(PegasusError::SerialFailure)
            }
        }
    }

    /// Query the firmware version string (PV command).
    pub fn firmware_version(&mut self) -> Result<String, PegasusError> {
        self.send_command(Command::FirmwareVersion as i32, None)
    }

    /// Query all power and environmental sensor readings (PA command).
    pub fn power_and_sensor_readings(&mut self) -> Result<PowerAndSensorReadings, PegasusError> {
        let resp = self.send_command(Command::PowerAndSensorReadings as i32, None)?;
        let chunks: Vec<&str> = resp.split(':').collect();
        if chunks.len() < 13 {
            error!("Unexpected PA response: {resp}");
            return Err(PegasusError::BadAnswer);
        }
        Ok(PowerAndSensorReadings {
            voltage: chunks[1]
                .parse()
                .map_err(|_| PegasusError::VoltageParsing)?,
            // Raw value must be divided by 65 per spec for compatibility with PPB.
            current_12v: chunks[2].parse::<f32>().unwrap_or(0.0) / 65.0,
            temperature: chunks[3]
                .parse()
                .map_err(|_| PegasusError::TemperatureParsing)?,
            humidity: chunks[4]
                .parse()
                .map_err(|_| PegasusError::HumidityParsing)?,
            dewpoint: chunks[5]
                .parse()
                .map_err(|_| PegasusError::DewPointParsing)?,
            quadport_status: chunks[6] == "1",
            adj_output_status: chunks[7] == "1",
            dew_a_power: chunks[8]
                .parse()
                .map_err(|_| PegasusError::DewPowerParsing)?,
            dew_b_power: chunks[9]
                .parse()
                .map_err(|_| PegasusError::DewPowerParsing)?,
            autodew: chunks[10] == "1",
            pwr_warn: chunks[11] == "1",
            adj_output_voltage: chunks[12]
                .parse()
                .map_err(|_| PegasusError::AdjOutParsing)?,
        })
    }

    /// Query power consumption statistics (PS command).
    pub fn power_consumption_stats(&mut self) -> Result<PowerConsumptionStats, PegasusError> {
        let resp = self.send_command(Command::PowerConsumAndStats as i32, None)?;
        let chunks: Vec<&str> = resp.split(':').collect();
        if chunks.len() < 5 {
            error!("Unexpected PS response: {resp}");
            return Err(PegasusError::BadAnswer);
        }
        Ok(PowerConsumptionStats {
            average_amps: chunks[1].parse().map_err(|_| PegasusError::AvgAmpParsing)?,
            amp_hours: chunks[2]
                .parse()
                .map_err(|_| PegasusError::AmpHourParsing)?,
            watt_hours: chunks[3]
                .parse()
                .map_err(|_| PegasusError::WattHourParsing)?,
            uptime_ms: chunks[4].parse().map_err(|_| PegasusError::UptimeParsing)?,
        })
    }

    /// Query per-channel current metrics (PC command).
    pub fn power_metrics(&mut self) -> Result<PowerMetrics, PegasusError> {
        let resp = self.send_command(Command::PowerMetrics as i32, None)?;
        let chunks: Vec<&str> = resp.split(':').collect();
        if chunks.len() < 6 {
            error!("Unexpected PC response: {resp}");
            return Err(PegasusError::BadAnswer);
        }
        Ok(PowerMetrics {
            total_current: chunks[1]
                .parse()
                .map_err(|_| PegasusError::TotalCurrentParsing)?,
            current_12v_output: chunks[2].parse().map_err(|_| PegasusError::Out12vParsing)?,
            current_dew_a: chunks[3]
                .parse()
                .map_err(|_| PegasusError::DewPowerParsing)?,
            current_dew_b: chunks[4]
                .parse()
                .map_err(|_| PegasusError::DewPowerParsing)?,
            uptime_ms: chunks[5].parse().map_err(|_| PegasusError::UptimeParsing)?,
        })
    }

    /// Set DewA heater PWM duty cycle (P3: command, range 0-255).
    pub fn set_dew1_power(&mut self, val: u8) -> Result<(), PegasusError> {
        self.send_command(Command::Dew1Power as i32, Some(val.to_string()))?;
        Ok(())
    }

    /// Set DewB heater PWM duty cycle (P4: command, range 0-255).
    pub fn set_dew2_power(&mut self, val: u8) -> Result<(), PegasusError> {
        self.send_command(Command::Dew2Power as i32, Some(val.to_string()))?;
        Ok(())
    }

    /// Set the adjustable output voltage/state (P2: command).
    ///
    /// Accepts 0/1 for off/on, or a specific voltage: 3, 5, 7, 8, 9, 12.
    pub fn set_adj_output(&mut self, val: u8) -> Result<(), PegasusError> {
        self.send_command(Command::Adj12VOutput as i32, Some(val.to_string()))?;
        Ok(())
    }

    /// Switch the quad 12V outputs on or off (P1: command).
    pub fn set_quadport_status(&mut self, on: bool) -> Result<(), PegasusError> {
        let arg = if on { "1" } else { "0" };
        self.send_command(Command::QuadPortStatus as i32, Some(arg.to_string()))?;
        Ok(())
    }

    /// Reboot the device and reload its firmware (PF command).
    pub fn reboot(&mut self) -> Result<(), PegasusError> {
        self.send_command(Command::Reboot as i32, None)?;
        Ok(())
    }
}
