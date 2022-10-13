//! # Control
//!
//! Control contains structures related to sending and receiving messages over PMD control point.
//!

use crate::{find_characteristic, Error, H10MeasurementType, PolarResult};

use btleplug::api::{Characteristic, Peripheral as _, WriteType};
use btleplug::platform::Peripheral;
use uuid::Uuid;

/// Polar Measurement Data Control Point (Read | Write | Indicate).
const PMD_CP_UUID: Uuid = Uuid::from_u128(0xfb005c81_02e7_f387_1cad_8acd2d8df0c8);
/// Polar Measurement Data... Data (Notify).
const PMD_DATA_UUID: Uuid = Uuid::from_u128(0xfb005c82_02e7_f387_1cad_8acd2d8df0c8);

/// Command options to write to the control point.
#[derive(Debug, PartialEq, Eq)]
pub enum ControlPointCommand {
    /// Do nothing.
    Null = 0,
    /// Get measurement settings for your data type.
    GetMeasurementSettings,
    /// Start measurement for all data types.
    RequestMeasurementStart,
    /// Stop all measurements in `PolarSensor.data_type`.
    StopMeasurement,
}

impl TryFrom<u8> for ControlPointCommand {
    type Error = ();

    fn try_from(val: u8) -> Result<Self, ()> {
        match val {
            0 => Ok(Self::Null),
            1 => Ok(Self::GetMeasurementSettings),
            2 => Ok(Self::RequestMeasurementStart),
            3 => Ok(Self::StopMeasurement),
            _ => {
                println!("Invalid ControlPointCommand {}", val);
                Err(())
            }
        }
    }
}

/// Response code returned after a write to PMD control point.
#[derive(Debug, PartialEq, Eq)]
pub enum ControlPointResponseCode {
    /// Command was successful.
    Success = 0,
    /// Control point command is not supported by device.
    InvalidOpCode,
    /// Device does not know the specified measurement type.
    InvalidMeasurementType,
    /// This measurement is not supported by device.
    NotSupported,
    /// Given length does not match the received data.
    InvalidLength,
    /// Contains parameters that prevent successful handling of request.
    InvalidParameter,
    /// Device is already in the requested state.
    AlreadyInState,
    /// Requested resolution is not supported by device.
    InvalidResolution,
    /// Requested sample rate is not supported by device.
    InvalidSampleRate,
    /// Requested range is not supported.
    InvalidRange,
    /// Connection MTU does not match device required MTU.
    InvalidMTU,
    /// Request contains invalid number of channels.
    InvalidNumberOfChannels,
    /// Device is in invalid state.
    InvalidState,
    /// Device is in charger and does not support requests.
    DeviceInCharger,
}

impl TryFrom<u8> for ControlPointResponseCode {
    type Error = ();

    fn try_from(val: u8) -> Result<Self, ()> {
        match val {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidOpCode),
            2 => Ok(Self::InvalidMeasurementType),
            3 => Ok(Self::NotSupported),
            4 => Ok(Self::InvalidLength),
            5 => Ok(Self::InvalidParameter),
            6 => Ok(Self::AlreadyInState),
            7 => Ok(Self::InvalidResolution),
            8 => Ok(Self::InvalidSampleRate),
            9 => Ok(Self::InvalidRange),
            10 => Ok(Self::InvalidMTU),
            11 => Ok(Self::InvalidNumberOfChannels),
            12 => Ok(Self::InvalidState),
            13 => Ok(Self::DeviceInCharger),
            _ => {
                println!("Invalid ControlPointResponseCode {}", val);
                Err(())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
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

    fn try_from(val: u8) -> Result<Self, ()> {
        match val {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidHandle),
            2 => Ok(Self::ReadNotPermitted),
            3 => Ok(Self::WriteNotPermitted),
            4 => Ok(Self::InvalidPdu),
            5 => Ok(Self::InsufficientAuthentication),
            6 => Ok(Self::RequestNotSupported),
            7 => Ok(Self::InvalidOffset),
            8 => Ok(Self::InsufficientAuthorization),
            9 => Ok(Self::PrepareQueueFull),
            10 => Ok(Self::AttributeNotFound),
            11 => Ok(Self::AttributeNotLong),
            12 => Ok(Self::InsufficientEncryptionKeySize),
            13 => Ok(Self::InsufficientAttributeValueLength),
            14 => Ok(Self::UnlikelyError),
            15 => Ok(Self::InsufficientEncryption),
            16 => Ok(Self::UnsupportedGroupType),
            17 => Ok(Self::InsufficientResources),
            _ => {
                println!("Invalid ResponseCode {}", val);
                Err(())
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SettingType {
    SampleRate,
    Resolution,
    Range,
}

impl SettingType {
    const fn from(byte: u8) -> Self {
        match byte {
            0x00 => Self::SampleRate,
            0x01 => Self::Resolution,
            _ => Self::Range,
        }
    }
}

enum PmdByteType {
    Setting,
    ArrLen,
    Data,
}

/// Struct to store the settings for a specific stream on your device.
#[derive(Debug, PartialEq, Eq)]
pub struct StreamSettings {
    ty: H10MeasurementType,
    resolution: u8,
    range: Option<Vec<u8>>,
    sample_rate: Vec<u8>,
}

impl StreamSettings {
    /// Create new stream settings.
    pub fn new(resp: &ControlResponse) -> PolarResult<Self> {
        if *resp.opcode() != ControlPointCommand::GetMeasurementSettings {
            return Err(Error::WrongResponse);
        }

        let mut resolution: u8 = 0;
        let mut ranges: Vec<u8> = vec![];
        let mut sample_rate: Vec<u8> = vec![];

        let mut setting: SettingType = SettingType::from(resp.parameters[0]);
        let mut next_byte: PmdByteType = PmdByteType::ArrLen;
        let mut len_remaining = 0u8;

        let mut data = resp.parameters()[1..].iter();

        while let Some(i) = data.next() {
            match next_byte {
                PmdByteType::Setting => {
                    setting = SettingType::from(*i);
                    next_byte = PmdByteType::ArrLen;
                }
                PmdByteType::ArrLen => {
                    len_remaining = *i;
                    next_byte = PmdByteType::Data;
                }
                PmdByteType::Data => {
                    match setting {
                        SettingType::SampleRate => {
                            sample_rate.push(*i);
                            let _ = data.next().unwrap();
                        }
                        SettingType::Resolution => {
                            resolution = *i;
                            let _ = data.next().unwrap();
                        }
                        SettingType::Range => {
                            ranges.push(*i);
                            let _ = data.next().unwrap();
                        }
                    }

                    len_remaining -= 1;
                    if len_remaining == 0 {
                        next_byte = PmdByteType::Setting;
                    }
                }
            }
        }

        let range = if !ranges.is_empty() {
            Some(ranges)
        } else {
            None
        };

        Ok(Self {
            ty: *resp.data_type(),
            resolution,
            range,
            sample_rate,
        })
    }

    /// Getter for the resolution (in bits).
    pub const fn resolution(&self) -> u8 {
        self.resolution
    }

    /// Getter for range (ACC only) (in G).
    pub const fn range(&self) -> &Option<Vec<u8>> {
        &self.range
    }

    /// Getter for sample rates (in Hz).
    pub const fn sample_rate(&self) -> &Vec<u8> {
        &self.sample_rate
    }
}

/// Store data returned from the device after a write to the control point.
#[derive(Debug)]
pub struct ControlResponse {
    op_code: ControlPointCommand,
    measurement_type: H10MeasurementType,
    status: ControlPointResponseCode,
    parameters: Vec<u8>,
}

impl ControlResponse {
    /// Create new [`ControlResponse`].
    pub async fn new(data: Vec<u8>) -> PolarResult<Self> {
        // We need at least 4 bytes for a complete packet
        if data.len() < 4 {
            return Err(Error::InvalidData);
        }
        // check that our response is a control point response
        if data[0] != 0xf0 {
            return Err(Error::InvalidData);
        }
        let op_code = ControlPointCommand::try_from(data[1]).map_err(|_| Error::InvalidData)?;
        let measurement_type =
            H10MeasurementType::try_from(data[2]).map_err(|_| Error::InvalidData)?;
        let status = ControlPointResponseCode::try_from(data[3]).map_err(|_| Error::InvalidData)?;
        let parameters = if data.len() > 5 {
            data[5..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            op_code,
            measurement_type,
            status,
            parameters,
        })
    }

    /// Return extra parameters of this response.
    pub const fn parameters(&self) -> &Vec<u8> {
        &self.parameters
    }

    /// Return op code of this response.
    pub const fn opcode(&self) -> &ControlPointCommand {
        &self.op_code
    }

    /// Get measurement type.
    pub const fn data_type(&self) -> &H10MeasurementType {
        &self.measurement_type
    }

    /// Get response status.
    pub const fn status(&self) -> &ControlPointResponseCode {
        &self.status
    }
}

/// Struct that has access to the PMD control point point and PMD data.
#[derive(Debug, PartialEq, Eq)]
pub struct ControlPoint {
    control_point: Characteristic,
    measurement_data: Characteristic,
}

impl ControlPoint {
    /// Create new [`ControlPoint`].
    pub async fn new(device: &Peripheral) -> PolarResult<Self> {
        let control_point = find_characteristic(device, PMD_CP_UUID).await?;
        let measurement_data = find_characteristic(device, PMD_DATA_UUID).await?;

        Ok(Self {
            control_point,
            measurement_data,
        })
    }

    /// Send command to [`ControlPoint`].
    pub async fn send_command(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<()> {
        self.write(device, data).await?;

        Ok(())
    }

    async fn write(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<()> {
        device
            .write(&self.control_point, &data, WriteType::WithResponse)
            .await
            .map_err(Error::BleError)
    }

    /// Read data from [`ControlPoint`] (for reading the features of a device).
    pub async fn read(&self, device: &Peripheral) -> PolarResult<Vec<u8>> {
        device
            .read(&self.control_point)
            .await
            .map_err(Error::BleError)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // for async testing
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn settings_ecg() {
        let norm = StreamSettings {
            ty: H10MeasurementType::Ecg,
            resolution: 14,
            range: None,
            sample_rate: vec![130],
        };

        let data = aw!(ControlResponse::new(vec![
            0xf0, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x82, 0x00, 0x01, 0x01, 0x0e, 0x00
        ]))
        .unwrap();

        assert_eq!(norm, StreamSettings::new(&data).unwrap());
    }

    #[test]
    fn settings_acc() {
        let norm = StreamSettings {
            ty: H10MeasurementType::Acc,
            resolution: 16,
            range: Some(vec![2, 4, 8]),
            sample_rate: vec![25, 50, 100, 200],
        };

        let data = aw!(ControlResponse::new(vec![
            0xf0, 0x01, 0x02, 0x00, 0x00, 0x00, 0x04, 0x19, 0x00, 0x32, 0x00, 0x64, 0x00, 0xC8,
            0x00, 0x01, 0x01, 0x10, 0x00, 0x02, 0x03, 0x02, 0x00, 0x04, 0x00, 0x08, 0x00
        ]))
        .unwrap();

        assert_eq!(norm, StreamSettings::new(&data).unwrap());
    }
}
