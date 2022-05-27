use tonic::{transport::Server, Request, Response, Status};

use lightspeed_astro::devices::actions::DeviceActions;
use lightspeed_astro::devices::ProtoDevice;
use lightspeed_astro::props::{SetPropertyRequest, SetPropertyResponse};
use lightspeed_astro::request::GetDevicesRequest;
use lightspeed_astro::response::GetDevicesResponse;
use lightspeed_astro::server::astro_service_server::{AstroService, AstroServiceServer};
use log::{debug, error, info};

pub mod ppba;
use astrotools::AstronomicalDevice;
use env_logger::Env;
use pegasus_astro::utils::look_for_devices;
use ppba::PowerBoxDevice;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default, Clone)]
struct PPBADriver {
    devices: Arc<Mutex<Vec<PowerBoxDevice>>>,
}

impl PPBADriver {
    fn new() -> Self {
        let found = look_for_devices("PPBA");
        let mut devices: Vec<PowerBoxDevice> = Vec::new();
        for dev in found {
            let mut device_name = String::from("PegausPowerBoxAdvanced");
            debug!("name: {}", dev.0);
            debug!("info: {:?}", dev.1);

            if let Some(serial) = dev.1.serial_number {
                device_name = device_name + "-" + &serial
            }
            if let Some(device) = PowerBoxDevice::new(&device_name, &dev.0, 9600, 500) {
                devices.push(device)
            } else {
                error!("Cannot start communication with {}", &device_name);
            }
        }
        Self {
            devices: Arc::new(Mutex::new(devices)),
        }
    }
}

#[tonic::async_trait]
impl AstroService for PPBADriver {
    async fn get_devices(
        &self,
        request: Request<GetDevicesRequest>,
    ) -> Result<Response<GetDevicesResponse>, Status> {
        debug!(
            "Got a request to query devices from {:?}",
            request.remote_addr()
        );

        if self.devices.lock().unwrap().is_empty() {
            let reply = GetDevicesResponse { devices: vec![] };
            Ok(Response::new(reply))
        } else {
            let mut devices = Vec::new();
            for device in self.devices.lock().unwrap().iter() {
                let d = ProtoDevice {
                    id: device.get_id().to_string(),
                    name: device.get_name().to_owned(),
                    address: device.get_address().to_owned(),
                    baud: device.baud as i32,
                    family: 0,
                    properties: device.properties.to_owned(),
                };
                devices.push(d);
            }
            let reply = GetDevicesResponse { devices: devices };
            Ok(Response::new(reply))
        }
    }

    async fn set_property(
        &self,
        request: Request<SetPropertyRequest>,
    ) -> Result<Response<SetPropertyResponse>, Status> {
        info!(
            "Got a request to set a property from {:?}",
            request.remote_addr()
        );
        let message = request.get_ref();
        debug!("device_id: {:?}", message.device_id);

        if message.device_id == "" || message.property_name == "" || message.property_value == "" {
            return Ok(Response::new(SetPropertyResponse {
                status: DeviceActions::InvalidValue as i32,
            }));
        };

        // TODO: return case if no devices match
        for d in self.devices.lock().unwrap().iter_mut() {
            if d.get_id().to_string() == message.device_id {
                info!(
                    "Updating property {} for {} to {}",
                    message.property_name, message.device_id, message.property_value,
                );

                if let Err(e) = d.update_property(&message.property_name, &message.property_value) {
                    info!(
                        "Updating property {} for {} failed with reason: {:?}",
                        message.property_name, message.device_id, e
                    );
                    return Ok(Response::new(SetPropertyResponse { status: e as i32 }));
                }
            }
        }

        let reply = SetPropertyResponse {
            status: DeviceActions::Ok as i32,
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Default to log level INFO if LS_LOG_LEVEL is not set as
    // an env var
    let env = Env::default().filter_or("LS_LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    // Reflection service
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(lightspeed_astro::proto::FD_DESCRIPTOR_SET)
        .build()
        .unwrap();

    let addr = "127.0.0.1:50051".parse().unwrap();
    let driver = PPBADriver::new();

    let devices = Arc::clone(&driver.devices);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let mut devices_list = devices.lock().unwrap();
            for device in devices_list.iter_mut() {
                device.fetch_props();
            }
        }
    });

    info!("PPBADriver process listening on {}", addr);
    Server::builder()
        .add_service(reflection_service)
        .add_service(AstroServiceServer::new(driver))
        .serve(addr)
        .await?;
    Ok(())
}
