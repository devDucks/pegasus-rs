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

use serde::{Deserialize, Serialize};
use tokio::{signal, task};
use uuid::Uuid;

use rumqttc::ClientError;
use std::collections::HashMap;

type PPBA = Arc<RwLock<PegasusPowerBox>>;

#[derive(Default, Clone)]
struct PPBADriver {
    devices: HashMap<String, PPBA>,
}

impl PPBADriver {
    fn new() -> Self {
        let found = look_for_devices("PPBA");
        let mut devices: HashMap<String, PPBA> = HashMap::new();

        for dev in found {
            let mut device_name = String::from("PegausPowerBoxAdvanced");
            debug!("name: {}", dev.0);
            debug!("info: {:?}", dev.1);

            if let Some(serial) = dev.1.serial_number {
                device_name = device_name + "-" + &serial
            }
            let device: PegasusPowerBox = PegasusPowerBox::new(&device_name, &dev.0, 9600, 500);
            devices.insert(device.id.to_string(), Arc::new(RwLock::new(device)));
        }
        Self { devices }
    }
}

async fn subscribe(client: AsyncClient, ids: &Vec<Uuid>) -> Result<(), ClientError> {
    for id in ids {
        client
            .subscribe(
                format!("{}", format_args!("devices/{}/update", &id)),
                QoS::ExactlyOnce,
            )
            .await?
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum PropValue {
    Int(u32),
    Bool(bool),
    Str(String),
    Float(f32),
}

#[derive(Debug, Serialize, Deserialize)]
/// Struct to serialize an update property request coming from MQTT
struct UpdatePropertyRequest {
    prop_name: String,
    value: PropValue,
}

#[tokio::main]
async fn main() {
    //    console_subscriber::init();
    let env = Env::default().filter_or("LS_LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let driver = PPBADriver::new();

    if driver.devices.is_empty() {
        warn!("No PPBA found on the system, exiting");
        std::process::exit(0)
    }

    let mut mqttoptions = MqttOptions::new("pegasus_ppba", "127.0.0.1", 1883);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    let mut devices_id = Vec::with_capacity(driver.devices.len());

    for (_, d) in driver.devices.iter() {
        devices_id.push(d.read().unwrap().id)
    }

    subscribe(client.clone(), &devices_id).await.unwrap();

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

    for (_, d) in driver.devices.iter() {
        let device = Arc::clone(d);
        let c = client.clone();
        task::spawn(async move {
            let d_id = device.read().unwrap().id;
            loop {
                let now = Instant::now();
                device.write().unwrap().fetch_props();
                let serialized = serde_json::to_string(&*device.read().unwrap()).unwrap();
                c.publish(
                    format!("{}", format_args!("devices/{}", &d_id)),
                    QoS::AtLeastOnce,
                    false,
                    serialized,
                )
                .await
                .unwrap();
                let elapsed = now.elapsed();
                info!("Refreshed and publishing state took: {:.2?}", elapsed);
                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        });
    }

    while let Ok(event) = eventloop.poll().await {
        debug!("Received = {:?}", event);
        match event {
            Incoming(inc) => match inc {
                Publish(data) => {
                    // All topics are in the form of devices/{UUID}/{action} so let's
                    // take advantage of this fact and avoid a string split
                    let device_id = &data.topic[8..44];
                    let topic = &data.topic[45..data.topic.len()];
                    let device = driver.devices.get(device_id).unwrap();
                    match topic {
                        "update" => {
                            let req: UpdatePropertyRequest =
                                serde_json::from_slice(&data.payload).unwrap();

                            match req.value {
                                PropValue::Bool(v) => {
                                    info!("Received bool: {:?}", req);
                                    match &req.prop_name[..] {
                                        "quadport_status" => {
                                            info!("Updating quadport_status");
                                            device.write().unwrap().set_adjustable_output(v);
                                        }
					"reboot" => {
					    info!("Issuing a reboot");
					    device.write().unwrap().reboot();
					}
                                        _ => (),
                                    }
                                }
                                PropValue::Int(v) => {
                                    info!("Received Number: {:?}", req);
                                    match &req.prop_name[..] {
                                        "dew_a_power" => {
                                            info!("Updating DewA PWM");
                                            device.write().unwrap().set_dew_pwm(0, v);
                                        }
                                        "dew_b_power" => {
                                            info!("Updating DewB PWM");
                                            device.write().unwrap().set_dew_pwm(1, v);
                                        }
                                        _ => (),
                                    }
                                }
                                _ => (),
                            }
                        }
                        _ => (),
                    }
                }
                _ => debug!("Incoming event: {:?}", inc),
            },
            Outgoing(out) => {
                debug!("Outgoing MQTT event: {:?}", out);
            }
        }
    }
}
