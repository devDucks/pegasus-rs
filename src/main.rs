use log::{debug, error};
use pegasus_rs::ppba::{AstronomicalDevice, PowerBoxDevice};
use pegasus_rs::utils;

fn print_device_table(devices: &Vec<PowerBoxDevice>) {
    for d in devices {
        println!("");
        println!("=======================================");
        println!("Device id: {}", d.id);
        println!("Device address: {}", d.address);
        println!("Device name: {}", d.name);
        println!("=======================================");
        println!("");
        println!(
            "-----------------------------------------------------------------------------------"
        );
        println!(
            "|          name           |    value        |    kind     |    permission         |"
        );
        println!(
            "-----------------------------------------------------------------------------------"
        );

        for prop in d.get_properties() {
            let name_padding = 25 - prop.name.len();
            let val_padding = 17 - prop.value.len();
            let kind_padding = 13 - prop.kind.len();
            let mut perm_padding = 15;

            match prop.permission {
                0 => (),
                _ => {
                    perm_padding = 14;
                }
            }
            let mut name = String::new();
            let mut val = String::new();
            let mut kind = String::new();
            let mut perm = String::new();

            for _ in 0..name_padding as usize {
                name += " ";
            }

            for _ in 0..val_padding as usize {
                val += " ";
            }

            for _ in 0..kind_padding as usize {
                kind += " ";
            }

            for _ in 0..perm_padding as usize {
                perm += " ";
            }

            println!(
                "|{}{}|{}{}|{}{}|{:?}{}|",
                prop.name, name, prop.value, val, prop.kind, kind, prop.permission, perm
            );
        }
        println!(
            "-----------------------------------------------------------------------------------"
        );
    }
}

fn main() {
    env_logger::init();
    let found = utils::look_for_devices("PPBA");
    let mut devices: Vec<PowerBoxDevice> = Vec::new();

    if found.is_empty() {
        error!("No Pegasus PPBA found");
        return;
    } else {
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
    }

    print_device_table(&devices);

    let d = &mut devices[0];

    println!("How many V we should output from 12V out?");
    let mut out_12v = String::new();
    std::io::stdin()
        .read_line(&mut out_12v)
        .expect("Failed to read input");

    match d.update_property("adjustable_output", &out_12v[..&out_12v.len() - 1]) {
        Ok(_) => debug!("Prop adjustable_output updated correctly"),
        Err(e) => error!("Err: {:?}", e),
    }

    match d.update_property("quadport_status", "1") {
        Ok(_) => debug!("Prop quadport_status updated correctly"),
        Err(e) => error!("Err: {:?}", e),
    }

    println!("How much power we want to set the dewA A out? (in %)");
    let mut dew1 = String::new();
    std::io::stdin()
        .read_line(&mut dew1)
        .expect("Failed to read input");

    let mut dew1_val = 255.0 / 100.0 * dew1[..dew1.len() - 1].parse::<f32>().unwrap();
    dew1_val = dew1_val.round();

    match d.update_property("dew1_power", &dew1_val.to_string()) {
        Ok(_) => debug!("Prop dew1 power updated correctly"),
        Err(e) => error!("Err: {:?}", e),
    }

    match d.update_property("power_status_on_boot", "1111") {
        Ok(_) => debug!("Prop P status updated correctly"),
        Err(e) => error!("Err: {:?}", e),
    }

    // match d.update_property("reboot", "1") {
    //     Ok(_) => println!("Prop Reboot updated correctly"),
    //     Err(e) => println!("Err: {:?}", e),
    // }

    print_device_table(&devices);
}
