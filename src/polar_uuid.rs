use uuid::Uuid;

/// Battery notify stream
const BATTERY_LEVEL_UUID: Uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
/// Heart rate notify stream
const HEART_RATE_SERVICE_UUID: Uuid = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);
const BODY_LOCATION_UUID: Uuid = Uuid::from_u128(0x00002a38_0000_1000_8000_00805f9b34fb);

// const PMD_CP_UUID: Uuid = Uuid::from_u128(0xfb005c81_02e7_f387_1cad_8acd2d8df0c8);
// const PMD_DATA_UUID: Uuid = Uuid::from_u128(0xfb005c82_02e7_f387_1cad_8acd2d8df0c8);

const MODEL_NUMBER_STRING_UUID: Uuid = Uuid::from_u128(0x00002a24_0000_1000_8000_00805f9b34fb);
const MANUFACTURER_NAME_STRING_UUID: Uuid = Uuid::from_u128(0x00002a29_0000_1000_8000_00805f9b34fb);
const HARDWARE_REVISION_STRING_UUID: Uuid = Uuid::from_u128(0x00002a27_0000_1000_8000_00805f9b34fb);
const FIRMWARE_REVISION_STRING_UUID: Uuid = Uuid::from_u128(0x00002a26_0000_1000_8000_00805f9b34fb);
const SOFTWARE_REVISION_STRING_UUID: Uuid = Uuid::from_u128(0x00002a28_0000_1000_8000_00805f9b34fb);
const SERIAL_NUMBER_STRING_UUID: Uuid = Uuid::from_u128(0x00002a25_0000_1000_8000_00805f9b34fb);
const SYSTEM_ID_UUID: Uuid = Uuid::from_u128(0x00002a23_0000_1000_8000_00805f9b34fb);

pub enum NotifyUuid {
    BatteryLevel,
    HeartMeasurement,
}

pub enum StringUuid {
    BodyLocation,
    ModelNumber,
    ManufacturerName,
    HardwareRevision,
    FirmwareRevision,
    SoftwareRevision,
    SerialNumber,
    SystemId,
}

pub fn notify_uuid(kind: NotifyUuid) -> Uuid {
    match kind {
        NotifyUuid::BatteryLevel => BATTERY_LEVEL_UUID,
        NotifyUuid::HeartMeasurement => HEART_RATE_SERVICE_UUID,
    }
}

pub fn string_uuid(kind: StringUuid) -> Uuid {
    match kind {
        StringUuid::BodyLocation => BODY_LOCATION_UUID,
        StringUuid::ModelNumber => MODEL_NUMBER_STRING_UUID,
        StringUuid::ManufacturerName => MANUFACTURER_NAME_STRING_UUID,
        StringUuid::HardwareRevision => HARDWARE_REVISION_STRING_UUID,
        StringUuid::FirmwareRevision => FIRMWARE_REVISION_STRING_UUID,
        StringUuid::SoftwareRevision => SOFTWARE_REVISION_STRING_UUID,
        StringUuid::SerialNumber => SERIAL_NUMBER_STRING_UUID,
        StringUuid::SystemId => SYSTEM_ID_UUID,
    }
}