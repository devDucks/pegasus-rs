use astrotools::find_serial_devices;
use libpegasus::ppba::Ppba;
use log::error;

fn main() {
    env_logger::init();

    let found = find_serial_devices("PPBA");
    if found.is_empty() {
        error!("No Pegasus PPBA found");
        return;
    }

    let (port, info) = &found[0];
    let name = info.serial_number.as_deref().unwrap_or("unknown");
    println!("Connecting to {} (serial: {})", port, name);

    let mut ppba = match Ppba::new(port, 9600, 500) {
        Ok(p) => p,
        Err(_) => {
            error!("Cannot open {}", port);
            return;
        }
    };

    match ppba.power_and_sensor_readings() {
        Ok(r) => {
            println!("Voltage:     {:.2} V", r.voltage);
            println!("Temperature: {:.1} °C", r.temperature);
            println!("Humidity:    {:.1} %", r.humidity);
            println!("Dewpoint:    {:.1} °C", r.dewpoint);
            println!("Quad ports:  {}", r.quadport_status);
            println!(
                "Adj output:  {} V (on={})",
                r.adj_output_voltage, r.adj_output_status
            );
            println!("Dew A:       {}/255", r.dew_a_power);
            println!("Dew B:       {}/255", r.dew_b_power);
        }
        Err(_) => error!("Failed to read PA"),
    }

    println!("\nAdjustable output voltage? (3/5/7/8/9/12, or 0=off)");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).expect("read failed");
    if let Ok(v) = input.trim().parse::<u8>() {
        if let Err(_) = ppba.set_adj_output(v) {
            error!("Failed to set adj output");
        }
    }

    println!("Quad ports on? (1/0)");
    input.clear();
    std::io::stdin().read_line(&mut input).expect("read failed");
    if let Ok(v) = input.trim().parse::<u8>() {
        if let Err(_) = ppba.set_quadport_status(v != 0) {
            error!("Failed to set quad port status");
        }
    }

    println!("Dew A power % (0-100)?");
    input.clear();
    std::io::stdin().read_line(&mut input).expect("read failed");
    if let Ok(pct) = input.trim().parse::<f32>() {
        let duty = (255.0 * pct / 100.0).round() as u8;
        if let Err(_) = ppba.set_dew1_power(duty) {
            error!("Failed to set dew A power");
        }
    }

    println!("Dew B power % (0-100)?");
    input.clear();
    std::io::stdin().read_line(&mut input).expect("read failed");
    if let Ok(pct) = input.trim().parse::<f32>() {
        let duty = (255.0 * pct / 100.0).round() as u8;
        if let Err(_) = ppba.set_dew2_power(duty) {
            error!("Failed to set dew B power");
        }
    }

    println!("\n--- Final state ---");
    match ppba.power_and_sensor_readings() {
        Ok(r) => {
            println!("Voltage:     {:.2} V", r.voltage);
            println!("Temperature: {:.1} °C", r.temperature);
            println!("Quad ports:  {}", r.quadport_status);
            println!(
                "Adj output:  {} V (on={})",
                r.adj_output_voltage, r.adj_output_status
            );
            println!("Dew A:       {}/255", r.dew_a_power);
            println!("Dew B:       {}/255", r.dew_b_power);
        }
        Err(_) => error!("Failed to read final PA"),
    }
}
