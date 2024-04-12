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
use uuid::Uuid;

type PPBA = Arc<RwLock<PegasusPowerBox>>;

#[derive(Default, Clone)]
struct PPBADriver {
    devices: Vec<PPBA>,
}

impl PPBADriver {
    fn new() -> Self {
        let found = look_for_devices("PPBA");
        let mut devices: Vec<PPBA> = Vec::new();

        for dev in found {
            let mut device_name = String::from("PegausPowerBoxAdvanced");
            debug!("name: {}", dev.0);
            debug!("info: {:?}", dev.1);

            if let Some(serial) = dev.1.serial_number {
                device_name = device_name + "-" + &serial
            }
            let device = Arc::new(RwLock::new(PegasusPowerBox::new(
                &device_name,
                &dev.0,
                9600,
                500,
            )));
            devices.push(device);
        }
        Self { devices }
    }
}

async fn subscribe(client: AsyncClient, ids: &Vec<Uuid>) {
    for id in ids {
        client
            .subscribe(
                format!("{}", format_args!("devices/{}/update", &id)),
                QoS::AtLeastOnce,
            )
            .await
            .unwrap();
    }
}

#[tokio::main]
async fn main() {
    console_subscriber::init();
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

    for d in &driver.devices {
        devices_id.push(d.read().unwrap().id)
    }

    subscribe(client.clone(), &devices_id).await;

    for d in &driver.devices {
        let device = Arc::clone(d);

        tokio::spawn(async move {
            signal::ctrl_c().await.unwrap();
            debug!("ctrl-c received!");
            std::process::exit(0);
        });
    }

    for d in &driver.devices {
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
                debug!("Refreshed and publishing state took: {:.2?}", elapsed);
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
                    match &data.topic[45..data.topic.len()] {
                        "update" => {
                            info!(
                                "received message from topic: {}\nmessage: {:?}",
                                &data.topic, &data.payload
                            );
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
