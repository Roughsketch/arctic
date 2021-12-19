use crate::{PolarResult, Error, find_characteristic};

use btleplug::api::{Characteristic, Peripheral as _, WriteType};
use btleplug::platform::Peripheral;
use uuid::Uuid;

/// Polar Measurement Data Control Point (Read | Write | Indicate)
const PMD_CP_UUID: Uuid = Uuid::from_u128(0xfb005c81_02e7_f387_1cad_8acd2d8df0c8);
/// Polar Measurement Data... Data (Notify)
const PMD_DATA_UUID: Uuid = Uuid::from_u128(0xfb005c82_02e7_f387_1cad_8acd2d8df0c8);

#[derive(Debug, PartialEq)]
enum ControlPointCommand {
    Null = 0,
    GetMeasurementSettings,
    RequestMeasurementStart,
    StopMeasurement,
    GetSdkModeMeasurementSettings,
}

impl TryFrom<u8> for ControlPointCommand {
    type Error = ();

    fn try_from(val: u8) -> Result<ControlPointCommand, ()> {
        match val {
            0 => Ok(ControlPointCommand::Null),
            1 => Ok(ControlPointCommand::GetMeasurementSettings),
            2 => Ok(ControlPointCommand::RequestMeasurementStart),
            3 => Ok(ControlPointCommand::StopMeasurement),
            4 => Ok(ControlPointCommand::GetSdkModeMeasurementSettings),
            _ => {
                println!("Invalid ControlPointCommand {}", val);
                Err(())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum ControlPointResponseCode {
    Success = 0,
    InvalidOpCode,
    InvalidMeasurementType,
    NotSupported,
    InvalidLength,
    InvalidParameter,
    AlreadyInState,
    InvalidResolution,
    InvalidSampleRate,
    InvalidRange,
    InvalidMTU,
    InvalidNumberOfChannels,
    InvalidState,
    DeviceInCharger,
}

impl TryFrom<u8> for ControlPointResponseCode {
    type Error = ();

    fn try_from(val: u8) -> Result<ControlPointResponseCode, ()> {
        match val {
            0 => Ok(ControlPointResponseCode::Success),
            1 => Ok(ControlPointResponseCode::InvalidOpCode),
            2 => Ok(ControlPointResponseCode::InvalidMeasurementType),
            3 => Ok(ControlPointResponseCode::NotSupported),
            4 => Ok(ControlPointResponseCode::InvalidLength),
            5 => Ok(ControlPointResponseCode::InvalidParameter),
            6 => Ok(ControlPointResponseCode::AlreadyInState),
            7 => Ok(ControlPointResponseCode::InvalidResolution),
            8 => Ok(ControlPointResponseCode::InvalidSampleRate),
            9 => Ok(ControlPointResponseCode::InvalidRange),
            10 => Ok(ControlPointResponseCode::InvalidMTU),
            11 => Ok(ControlPointResponseCode::InvalidNumberOfChannels),
            12 => Ok(ControlPointResponseCode::InvalidState),
            13 => Ok(ControlPointResponseCode::DeviceInCharger),
            _ => {
                println!("Invalid ControlPointResponseCode {}", val);
                Err(())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum MeasurementType {
    Ecg,
    Ppg,
    Acc,
    Ppi,
    Bioz,
    Gyro,
    Mgn,
    Barometer,
    Ambient,
    SdkMode,
}

impl TryFrom<u8> for MeasurementType {
    type Error = ();

    fn try_from(val: u8) -> Result<MeasurementType, ()> {
        match val {
            0 => Ok(MeasurementType::Ecg),
            1 => Ok(MeasurementType::Ppg),
            2 => Ok(MeasurementType::Acc),
            3 => Ok(MeasurementType::Ppi),
            4 => Ok(MeasurementType::Bioz),
            5 => Ok(MeasurementType::Gyro),
            6 => Ok(MeasurementType::Mgn),
            7 => Ok(MeasurementType::Barometer),
            8 => Ok(MeasurementType::Ambient),
            9 => Ok(MeasurementType::SdkMode),
            _ => {
                println!("Invalid MeasurementType {}", val);
                Err(())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum ResponseCode {
    Success,
    InvalidHandle,
    ReadNotPermitted,
    WriteNotPermitted,
    InvalidPdu,
    InsufficientAuthentication,
    RequestNotSupported,
    InvalidOffset,
    InsufficientAuthorization,
    PrepareQueueFull,
    AttributeNotFound,
    AttributeNotLong,
    InsufficientEncryptionKeySize,
    InsufficientAttributeValueLength,
    UnlikelyError,
    InsufficientEncryption,
    UnsupportedGroupType,
    InsufficientResources,
}

impl TryFrom<u8> for ResponseCode {
    type Error = ();

    fn try_from(val: u8) -> Result<ResponseCode, ()> {
        match val {
            0 => Ok(ResponseCode::Success),
            1 => Ok(ResponseCode::InvalidHandle),
            2 => Ok(ResponseCode::ReadNotPermitted),
            3 => Ok(ResponseCode::WriteNotPermitted),
            4 => Ok(ResponseCode::InvalidPdu),
            5 => Ok(ResponseCode::InsufficientAuthentication),
            6 => Ok(ResponseCode::RequestNotSupported),
            7 => Ok(ResponseCode::InvalidOffset),
            8 => Ok(ResponseCode::InsufficientAuthorization),
            9 => Ok(ResponseCode::PrepareQueueFull),
            10 => Ok(ResponseCode::AttributeNotFound),
            11 => Ok(ResponseCode::AttributeNotLong),
            12 => Ok(ResponseCode::InsufficientEncryptionKeySize),
            13 => Ok(ResponseCode::InsufficientAttributeValueLength),
            14 => Ok(ResponseCode::UnlikelyError),
            15 => Ok(ResponseCode::InsufficientEncryption),
            16 => Ok(ResponseCode::UnsupportedGroupType),
            17 => Ok(ResponseCode::InsufficientResources),
            _ => {
                println!("Invalid ResponseCode {}", val);
                Err(())
            }
        }
    }
}

#[derive(Debug)]
pub struct ControlResponse {
    response_code: ResponseCode,
    opcode: ControlPointCommand,
    measurement_type: MeasurementType,
    status: ControlPointResponseCode,
    parameters: Vec<u8>,
    more: bool,
}

impl ControlResponse {
    async fn new(data: Vec<u8>) -> PolarResult<ControlResponse> {
        // We need at least 4 bytes for a complete packet
        if data.len() < 4 {
            return Err(Error::InvalidData);
        }

        let response_code = ResponseCode::try_from(data[0]).map_err(|_| Error::InvalidData)?;
        let opcode = ControlPointCommand::try_from(data[1]).map_err(|_| Error::InvalidData)?;
        let measurement_type = MeasurementType::try_from(data[2]).map_err(|_| Error::InvalidData)?;
        let status = ControlPointResponseCode::try_from(data[3]).map_err(|_| Error::InvalidData)?;
        let mut parameters = Vec::new();
        let more = {
            if status == ControlPointResponseCode::Success {
                if data.len() > 5 {
                    parameters = data[5..].to_vec();
                }
                data.len() > 4 && data[4] != 0
            } else {
                false
            }
        };

        Ok(ControlResponse {
            response_code,
            opcode,
            measurement_type,
            status,
            parameters,
            more,
        })
    }

    fn add_parameters(&mut self, data: Vec<u8>) -> bool {
        if data[0] != 0 {
            self.parameters.append(&mut data[1..].to_vec());
        } else {
            self.more = false;
        }

        self.more
    }
}

#[derive(Debug)]
pub struct ControlPoint {
    control_point: Characteristic,
    measurement_data: Characteristic,
}

impl ControlPoint {
    pub async fn new(device: &Peripheral) -> PolarResult<ControlPoint> {
        let control_point = find_characteristic(device, PMD_CP_UUID).await?;
        let measurement_data = find_characteristic(device, PMD_DATA_UUID).await?;

        Ok(ControlPoint {
            control_point,
            measurement_data,
        })
    }

    pub async fn send_command(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<ControlResponse> {
        println!("Writing cmd: {:?}", data);
        self.write(device, data).await?;

        let response_data = self.read(device).await?;
        println!("Read response: {:?}", response_data);
        let mut response = ControlResponse::new(response_data).await?;

        while response.more {
            let response_data = self.read(device).await?;
            println!("Read response (more): {:?}", response_data);
            response.add_parameters(response_data);
        }

        Ok(response)
    }

    async fn write(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<()> {
        device.write(&self.control_point, &data, WriteType::WithResponse)
            .await
            .map_err(Error::BleError)
    }

    async fn read(&self, device: &Peripheral) -> PolarResult<Vec<u8>> {
        device.read(&self.control_point)
            .await
            .map_err(Error::BleError)
    }
}