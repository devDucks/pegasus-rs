pub mod utils;

pub mod common {
    use lightspeed_astro::props::Property;
    #[cfg(windows)]
    use serialport::COMPort;
    #[cfg(unix)]
    use serialport::TTYPort;
    use uuid::Uuid;

    pub struct PowerBoxDevice {
        id: Uuid,
        name: String,
        pub properties: Vec<Property>,
        address: String,
        pub baud: u32,
        #[cfg(unix)]
        pub port: TTYPort,
        #[cfg(windows)]
        pub port: COMPort,
    }
}
