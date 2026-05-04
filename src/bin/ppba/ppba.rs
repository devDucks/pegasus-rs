use astrotools::device::{DeviceType, LightspeedDevice};
use astrotools::properties::{
    Permission, Prop, PropValue, Property, PropertyErrorType, UpdatePropertyRequest,
};
use astrotools::LightspeedError;
use libpegasus::ppba::Ppba;
use libpegasus::PegasusError;
use log::{error, info};
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, SyncSender};
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
    #[serde(skip)]
    ppba: Ppba,
    fw_version: Property<String>,
    reboot: Property<bool>,
    input_voltage: Property<f32>,
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

impl PegasusPowerBox {
    pub fn new(
        name: &str,
        address: &str,
        baud: u32,
        timeout_ms: u64,
    ) -> Result<Self, PegasusError> {
        let ppba = Ppba::new(address, baud, timeout_ms)?;
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<PpbaCommand>(32);

        let mut dev = Self {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            address: address.to_owned(),
            baud,
            ppba,
            fw_version: Property::<String>::new("UNKNOWN".to_string(), Permission::ReadOnly),
            reboot: Property::<bool>::new(false, Permission::ReadWrite),
            input_voltage: Property::<f32>::new(0.0, Permission::ReadOnly),
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

        dev.update_firmware_version();
        dev.fetch_props();
        Ok(dev)
    }

    pub fn fetch_props(&mut self) {
        info!("Fetching properties for device {}", self.name);
        self.update_power_consumption_and_stats();
        self.update_power_metrics();
        self.update_power_and_sensor_readings();
    }

    fn update_firmware_version(&mut self) {
        match self.ppba.firmware_version() {
            Ok(fw) => {
                let _ = self.fw_version.update_int(fw);
            }
            Err(_) => (),
        }
    }

    fn update_power_consumption_and_stats(&mut self) {
        match self.ppba.power_consumption_stats() {
            Ok(stats) => {
                let _ = self.average_amps.update_int(stats.average_amps);
                let _ = self.amps_hours.update_int(stats.amp_hours);
                let _ = self.watt_hours.update_int(stats.watt_hours);
                let _ = self.uptime.update_int(stats.uptime_ms);
            }
            Err(_) => (),
        }
    }

    fn update_power_metrics(&mut self) {
        match self.ppba.power_metrics() {
            Ok(metrics) => {
                let _ = self.total_current.update_int(metrics.total_current);
                let _ = self
                    .current_12v_output
                    .update_int(metrics.current_12v_output);
                let _ = self.dew1_current.update_int(metrics.current_dew_a);
                let _ = self.dew2_current.update_int(metrics.current_dew_b);
            }
            Err(_) => (),
        }
    }

    fn update_power_and_sensor_readings(&mut self) {
        match self.ppba.power_and_sensor_readings() {
            Ok(r) => {
                let _ = self.input_voltage.update_int(r.voltage);
                let _ = self.current_12v_output.update_int(r.current_12v);
                let _ = self.temperature.update_int(r.temperature);
                let _ = self.humidity.update_int(r.humidity);
                let _ = self.quadport_status.update_int(r.quadport_status);
                let _ = self.adj_output_status.update_int(r.adj_output_status);
                let _ = self.dew1_power.update_int(r.dew_a_power);
                let _ = self.dew2_power.update_int(r.dew_b_power);
                let _ = self.autodew.update_int(r.autodew);
                let _ = self.pwr_warn.update_int(r.pwr_warn);
                let _ = self.adj_output.update_int(r.adj_output_voltage);
            }
            Err(_) => (),
        }
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
                self.ppba
                    .set_dew1_power(v)
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.dew1_power.update_int(v);
                Ok(())
            }
            "dew2_power" => {
                let v = u32::try_from(val)? as u8;
                self.ppba
                    .set_dew2_power(v)
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.dew2_power.update_int(v);
                Ok(())
            }
            "adj_output" => {
                let v = u32::try_from(val)? as u8;
                self.ppba
                    .set_adj_output(v)
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.adj_output.update_int(v);
                Ok(())
            }
            "quadport_status" => {
                let on = match val {
                    PropValue::Bool(b) => b,
                    PropValue::Int(i) => i != 0,
                    _ => {
                        return Err(LightspeedError::PropertyError(
                            PropertyErrorType::InvalidValue,
                        ))
                    }
                };
                self.ppba
                    .set_quadport_status(on)
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                let _ = self.quadport_status.update_int(on);
                Ok(())
            }
            "reboot" => {
                self.ppba
                    .reboot()
                    .map_err(|_| LightspeedError::DeviceConnectionError)?;
                Ok(())
            }
            _ => Err(LightspeedError::PropertyError(
                PropertyErrorType::InvalidValue,
            )),
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
