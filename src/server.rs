use tonic::{transport::Server, Request, Response, Status};

use lightspeed::devices::actions::DeviceActions;
use lightspeed::devices::ProtoDevice;
use lightspeed::props::{SetPropertyRequest, SetPropertyResponse};
use lightspeed::request::GetDevicesRequest;
use lightspeed::response::GetDevicesResponse;
use lightspeed::server::astro_service_server::{AstroService, AstroServiceServer};
use log::{debug, error};
use pegasus_rs::ppba::{AstronomicalDevice, PowerBoxDevice};
use pegasus_rs::utils::look_for_devices;
use std::sync::Arc;
use std::sync::Mutex;

use std::time::Duration;

#[derive(Default, Clone)]
struct PegasusServer {
    devices: Arc<Mutex<Vec<PowerBoxDevice>>>,
}

impl PegasusServer {
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
            if let Ok(device) = PowerBoxDevice::new(&device_name, &dev.0, 9600) {
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
impl AstroService for PegasusServer {
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
                    id: device.id.to_string(),
                    name: device.name.to_owned(),
                    address: device.address.to_owned(),
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
        debug!(
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
            if d.id.to_string() == message.device_id {
                debug!(
                    "Updating property {} for {} to {}",
                    message.property_name, message.device_id, message.property_value,
                );

                if let Err(e) = d.update_property(&message.property_name, &message.property_value) {
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
    env_logger::init();
    let addr = "127.0.0.1:50051".parse().unwrap();
    let pegasus_service = PegasusServer::new();

    let dvs = Arc::clone(&pegasus_service.devices);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let mut d = dvs.lock().unwrap();
            for x in d.iter_mut() {
                x.fetch_props();
            }
        }
    });

    println!("GreeterServer listening on {}", addr);
    Server::builder()
        .add_service(AstroServiceServer::new(pegasus_service))
        .serve(addr)
        .await?;
    Ok(())
}
