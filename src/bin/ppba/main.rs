use env_logger::Env;
use log::{error, warn};
use pegasus_astro::utils::look_for_devices;

pub mod ppba;
use ppba::PegasusPowerBox;

fn main() {
    env_logger::init_from_env(Env::default().filter_or("LS_LOG_LEVEL", "info"));

    let devices: Vec<PegasusPowerBox> = look_for_devices("PPBA")
        .into_iter()
        .filter_map(|(port, info)| {
            let mut name = "PegasusPowerBoxAdvanced".to_string();
            if let Some(serial) = info.serial_number {
                name = name + "-" + &serial;
            }
            PegasusPowerBox::new(&name, &port, 9600, 500)
                .map_err(|e| error!("Skipping {port}: {e}"))
                .ok()
        })
        .collect();

    if devices.is_empty() {
        warn!("No PPBA found on the system, exiting");
        std::process::exit(0);
    }

    astrotools::runner::run(
        devices,
        astrotools::runner::RunnerConfig {
            mqtt_client_id: "pegasus_ppba".to_string(),
            tick_interval_ms: 500,
            ..Default::default()
        },
    );
}
