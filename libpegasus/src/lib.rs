pub mod ppba;

pub enum PegasusError {
    OpenFailed,
    SerialFailure,
    SerialEncode,
    SerialTimeout,
    BadCommand,
    BadAnswer,
    VoltageParsing,
    DewPowerParsing,
    TemperatureParsing,
    HumidityParsing,
    DewPointParsing,
    AdjOutParsing,
    AvgAmpParsing,
    AmpHourParsing,
    WattHourParsing,
    UptimeParsing,
    TotalCurrentParsing,
    Out12vParsing,
}
