# pegasus-rs
Multiplatform drivers for pegasus equipment written in Rust.

This driver is meant to communicate with all pegasus powerboxes on all major platforms.

# Run locally (UNIX/Windows)
Be sure to have rust installed (if you don't have rust check [here](https://www.rust-lang.org/tools/install) and
simply run on your terminal/cmdshell `cargo run`

# Build a debug version of the program (UNIX/Windows)
in your terminal type `cargo build`

# Build an optimized version of the program AKA the version that will run for real (UNIX/Windows)
in your terminal type `cargo build --release`

# Pegasus PPBA protocol instructions

|Command|Description                                                                            |Response           |
|:-:    |:-:                                                                                    |:-:                |
|P#|Status|PPBA_OK|
|PE:bbbb|Set Power Status on boot.<br>Every number represents 1-4 power outputs.<br>(0=OFF, 1=ON).|PE:1               |
|P2:nn|ON/OFF Power 8V DSLR<br>(0=OFF, 1=ON)<br>n can also accept values of: 3, 5, 8, 9, 12 (Volts)|P2:nn              |
|P3:nnn|PWM Duty Cycle Power 5 (DewA)<br>X=0-255 (0-100%)|P3:nnn|
|P4:nnn|PWM Duty Cycle Power 6 (DewB)<br>X=0-255 (0-100%)|P4:nnn|
|PF|Reboot Device / Reload Firmware|[none]|
|PA|Print Power and Sensor Readings|[Check table below]|
|PS|Prints Power Consumption Statistics|PS:averageAmps:ampHours:wattHours:uptime_in_millisec|
|PC|Print Power Metrics|*Current is represented in Amps and does not require <br>conversion.<br>PC:total_current:current_12V_outputs:<br>current_dewA:current_dewB:uptime_in_ millisec|
|PR|Prints discovered I2C devices plugged to EXT port|PR:HDC:DHT:XS<br>if there is a discovered device command will output its name<br>HDC = temp/humidity sensor TI HDC1050<br>DHT =stock temp/humidity sensor AM2301<br>XS: eXternal Motor (stepper) Controller<br>|
|DA|(Auto) Dew Aggressiveness from 0 to 255 (210 default value)|DA:nnn|
|PD:b|Enable/Disable Auto Dew Feature (X=0,1)<br>PD:99 Reports Auto Dew Aggressiveness value|PD:nnn|
|PV|Firmware Version|n.n|
|PI|Reset I2C channel|PI:1|
|PL:b|OF/OFF Led Indicator (0=OFF, 1=ON)|PL:b|
