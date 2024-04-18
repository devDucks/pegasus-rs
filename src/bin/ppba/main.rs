use log::{debug, error, info, warn};

pub mod ppba;
use env_logger::Env;
use pegasus_astro::utils::look_for_devices;
use ppba::PegasusPowerBox;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use rumqttc::Event::{Incoming, Outgoing};
use rumqttc::Packet::Publish;
use rumqttc::{AsyncClient, MqttOptions, QoS};

use tokio::{signal, task};

use astrotools::properties::{PropValue, UpdatePropertyRequest};
use astrotools::LightspeedError;
use rumqttc::ClientError;
use std::collections::HashMap;

type PPBA = Arc<RwLock<PegasusPowerBox>>;

#[derive(Debug, Clone)]
struct PPBADriver {
    devices: HashMap<String, PPBA>,
    mqtt_client: AsyncClient,
}

impl PPBADriver {
    fn new(client: AsyncClient) -> Self {
        let mut driver = Self {
            devices: HashMap::new(),
            mqtt_client: client,
        };
        driver.find_devices();
        if driver.devices.is_empty() {
            warn!("No PPBA found on the system");
        }
        driver
    }

    fn remove_device(&mut self, dev_name: &str) {
        let _ = self.devices.remove(dev_name);
        warn!("Device disconnected: {}", dev_name);
    }

    fn add_device(&mut self, device_name: &String, port: &String) {
        let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, device_name.as_bytes()).to_string();

        if !self.devices.contains_key(&id) {
            if let Ok(device) = PegasusPowerBox::new(&device_name, port, 9600, 500) {
                info!("New device discovered: {}", &device_name);
                let id = device.id.to_string().clone();
                self.devices
                    .insert(device.id.to_string(), Arc::new(RwLock::new(device)));
                let _ = self.subscribe(&id);
                self.start_loop(&id);
            }
        }
    }

    fn start_loop(&self, device_id: &String) {
        let device = self.devices.get(device_id).unwrap().clone();
        let id = device_id.clone();
        let client = self.mqtt_client.clone();

        task::spawn(async move {
            loop {
                let now = Instant::now();
                if device.write().unwrap().fetch_props().is_ok() {
                    let serialized = serde_json::to_string(&*device.read().unwrap()).unwrap();
                    client
                        .publish(
                            format!("{}", format_args!("devices/{}", &id)),
                            QoS::AtLeastOnce,
                            false,
                            serialized,
                        )
                        .await
                        .unwrap();
                    let elapsed = now.elapsed();
                    info!("Refreshed and publishing state took: {:.2?}", elapsed);
                    tokio::time::sleep(Duration::from_millis(5000)).await;
                } else {
                    client
                        .publish(
                            format!("{}", format_args!("devices/{}/delete", &id)),
                            QoS::AtLeastOnce,
                            false,
                            vec![],
                        )
                        .await
                        .unwrap();
                    break;
                }
            }
        });
    }

    fn subscribe(&self, id: &String) -> Result<(), ClientError> {
        let client = self.mqtt_client.clone();
        let d_id = id.clone();
        tokio::spawn(async move {
            let _ = client
                .subscribe(
                    format!("{}", format_args!("devices/{}/update", &d_id)),
                    QoS::ExactlyOnce,
                )
                .await;
            let _ = client
                .subscribe(
                    format!("{}", format_args!("devices/{}/delete", &d_id)),
                    QoS::ExactlyOnce,
                )
                .await;
            let _ = client
                .subscribe(String::from("devices/ppba/new"), QoS::ExactlyOnce)
                .await;
        });
        Ok(())
    }

    fn find_devices(&mut self) {
        let found = look_for_devices("PPBA");
        for dev in found {
            let serial = dev.1.serial_number.clone().unwrap();
            let device_name = format!("PegausPowerBoxAdvanced-{}", &serial);
            debug!("info: {:?}", &dev);
            self.add_device(&device_name, &dev.0);
        }
    }
}

async fn notify_update_error(
    client: AsyncClient,
    id: &str,
    prop: &UpdatePropertyRequest,
    err: LightspeedError,
) -> Result<(), ClientError> {
    client
        .publish(
            format!("{}", format_args!("devices/{}/update/error", &id)),
            QoS::ExactlyOnce,
            false,
            serde_json::to_vec(&serde_json::json!({
            "prop_name": prop.prop_name,
            "value": prop.value,
            "error": err,
            }))
            .unwrap(),
        )
        .await?;
    Ok(())
}

fn update_property(req: &UpdatePropertyRequest, device: PPBA) -> Result<(), LightspeedError> {
    match req.value {
        PropValue::Bool(v) => {
            info!("Received bool: {:?}", req);
            match &req.prop_name[..] {
                "quadport_status" => {
                    info!("Updating quadport_status");
                    let mut d = device.write().unwrap();
                    d.set_adjustable_output(v)
                }
                "reboot" => {
                    info!("Issuing a reboot");
                    device.write().unwrap().reboot()
                }
                _ => {
                    warn!("Unknown property: {}", &req.prop_name[..]);
                    Ok(())
                }
            }
        }
        PropValue::Int(v) => {
            info!("Received Number: {:?}", req);
            match &req.prop_name[..] {
                "dew_a_power" => {
                    info!("Updating DewA PWM");
                    device.write().unwrap().set_dew_pwm(0, v)
                }
                "dew_b_power" => {
                    info!("Updating DewB PWM");
                    device.write().unwrap().set_dew_pwm(1, v)
                }
                _ => {
                    warn!("Unknown property: {}", &req.prop_name[..]);
                    Ok(())
                }
            }
        }
        _ => Ok(()),
    }
}

#[tokio::main]
async fn main() {
    //    console_subscriber::init();
    let env = Env::default().filter_or("LS_LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let mut mqttoptions = MqttOptions::new("pegasus_ppba", "127.0.0.1", 1883);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    let mut driver = PPBADriver::new(client.clone());

    match eventloop.poll().await {
        Err(rumqttc::ConnectionError::ConnectionRefused(_))
        | Err(rumqttc::ConnectionError::Io(_)) => {
            error!("The MQTT broker is not avialble, aborting");
            std::process::exit(0)
        }
        Err(e) => {
            error!("An error occured: {} - aborting", e);
            std::process::exit(0)
        }
        _ => (),
    }

    eventloop.network_options.set_connection_timeout(5);

    let c_client = client.clone();

    tokio::spawn(async move {
        signal::ctrl_c().await.unwrap();
        debug!("ctrl-c received!");
        c_client.disconnect().await.unwrap();
        std::process::exit(0);
    });

    while let Ok(event) = eventloop.poll().await {
        debug!("Received = {:?}", event);

        match event {
            Incoming(inc) => match inc {
                Publish(data) => {
                    // All topics are in the form of devices/{UUID}/{action} so let's
                    // take advantage of this fact and avoid a string split
                    let device_id = &data.topic[8..44];
                    let topic = &data.topic[45..data.topic.len()];
                    let device = driver.devices.get(device_id).unwrap().clone();

                    if topic == "update" {
                        let req: UpdatePropertyRequest =
                            serde_json::from_slice(&data.payload).unwrap();
                        match update_property(&req, device) {
                            Ok(()) => (),
                            Err(e) => {
                                error!("Update error: {e:?}");
                                match e {
                                    LightspeedError::IoError(ref _i) => {
                                        driver.remove_device(&device_id);
                                    }
                                    _ => (),
                                }
                                if notify_update_error(client.clone(), device_id, &req, e)
                                    .await
                                    .is_err()
                                {
                                    log::error!("Failed to send error message to broker")
                                }
                            }
                        }
                    } else if topic == "delete" {
                        info!("Delete message received");
                        driver.remove_device(&device_id);
                    } else if topic == "new" {
                        info!("Found new device");
                    } else {
                        warn!("Topic not managed: {}", &topic);
                    };
                }
                _ => debug!("Incoming event: {:?}", inc),
            },
            Outgoing(out) => {
                debug!("Outgoing MQTT event: {:?}", out);
            }
        }
    }
}
