use crate::{find_characteristic, Error, H10MeasurementType, PolarResult};

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

/* #[derive(Debug, PartialEq)]
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
}*/

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
    measurement_type: H10MeasurementType,
    status: ControlPointResponseCode,
    parameters: Vec<u8>,
    more: bool,
}

impl ControlResponse {
    pub async fn new(data: Vec<u8>) -> PolarResult<ControlResponse> {
        // We need at least 4 bytes for a complete packet
        if data.len() < 4 {
            return Err(Error::InvalidData);
        }

        let response_code = ResponseCode::try_from(data[0]).map_err(|_| Error::InvalidData)?;
        let opcode = ControlPointCommand::try_from(data[1]).map_err(|_| Error::InvalidData)?;
        let measurement_type =
            H10MeasurementType::try_from(data[2]).map_err(|_| Error::InvalidData)?;
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

    pub fn parameters(&self) -> &Vec<u8> {
        &self.parameters
    }
}

/// Struct that has access to the PMD control point point and PMD data
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

    pub async fn send_command(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<()> {
        println!("Writing cmd: {:?}", data);
        self.write(device, data).await?;

        /*let response_data = self.read(device).await?;
        println!("Read response: {:?}", response_data);
        let mut response = ControlResponse::new(response_data).await?;

        while response.more {
            let response_data = self.read(device).await?;
            println!("Read response (more): {:?}", response_data);
            response.add_parameters(response_data);
        }*/

        Ok(())
    }

    async fn write(&self, device: &Peripheral, data: Vec<u8>) -> PolarResult<()> {
        let response = device
            .write(&self.control_point, &data, WriteType::WithoutResponse)
            .await
            .map_err(Error::BleError);

        println!("response: {:?}", response.as_ref().unwrap());

        return response;
    }

    async fn read(&self, device: &Peripheral) -> PolarResult<Vec<u8>> {
        device
            .read(&self.control_point)
            .await
            .map_err(Error::BleError)
    }
}

/// Struct for reveiving measurement type data on PMD data
#[derive(Debug)]
pub struct PmdRead {
    data_type: H10MeasurementType,
    timestamp: u64,
    data: PmdData, // TODO: make this Vector of data, there can be multiple samples in one message
}

impl PmdRead {
    pub fn new(data_stream: Vec<u8>) -> PolarResult<PmdRead> {
        let data_type = H10MeasurementType::try_from(data_stream[0]);
        if let Err(_e) = data_type {
            return Err(Error::InvalidData);
        }
        let data_type = data_type.unwrap();
        let timestamp = u64::from_be_bytes(
            data_stream[1..9]
                .try_into()
                .expect("Timestamp slice could not be converted to u64"),
        );
        let frame_type = FrameType::try_from(data_stream[9])?;
        let data = match data_type {
            H10MeasurementType::Ecg => PmdData::Ecg(Ecg::new(&data_stream[10..].to_vec())?),
            H10MeasurementType::Ppg => PmdData::Ppg(Ppg::new(&data_stream[10..].to_vec())?),
            H10MeasurementType::Acc => PmdData::Acc(Acc::new(&data_stream[10..].to_vec(), frame_type)?),
            H10MeasurementType::Ppi => PmdData::Ppi(Ppi::new(&data_stream[10..].to_vec())?),
        };

        Ok(PmdRead {
            data_type,
            timestamp,
            data,
        })
    }
}

/// Enum to store which kind of data was received
#[derive(Debug)]
pub enum PmdData {
    Ecg(Ecg),
    Ppg(Ppg),
    Acc(Acc),
    Ppi(Ppi),
}

/// Struct to store ECG from the PMD data stream
#[derive(Debug)]
pub struct Ecg {
    ecg: i32,
}

impl Ecg {
    /// Convert data into ECG data
    fn new(data: &Vec<u8>) -> PolarResult<Ecg> {
        if data.len() < 3 {
            println!("ECG expects 3 bytes of data, got {}.", data.len());
            return Err(Error::InvalidLength);
        }
        let mut mag = [0u8; 4];
        mag[1..4].clone_from_slice(&data[..3]);

        Ok(Ecg {
            ecg: i32::from_be_bytes(mag),
        })
    }
}

/// Struct to store PPG from the PMD data stream
#[derive(Debug)]
pub struct Ppg {
    ppg0: i32,
    ppg1: i32,
    ppg2: i32,
    ambient: i32,
}

impl Ppg {
    /// Convert data into PPG data
    fn new(data: &Vec<u8>) -> PolarResult<Ppg> {
        if data.len() < 12 {
            println!("PPG expects 12 bytes of data, got {}", data.len());
            return Err(Error::InvalidLength);
        }

        // fix to convert to 4 byte arrays first, before converting to i32
        Ok(Ppg {
            ppg0: i32::from_be_bytes(
                data[..3]
                    .try_into()
                    .expect("Error converting data to PPG0."),
            ),
            ppg1: i32::from_be_bytes(
                data[3..6]
                    .try_into()
                    .expect("Error converting data to PPG1."),
            ),
            ppg2: i32::from_be_bytes(
                data[6..9]
                    .try_into()
                    .expect("Error converting data to PPG2."),
            ),
            ambient: i32::from_be_bytes(
                data[9..12]
                    .try_into()
                    .expect("Error converting data to PPG ambient."),
            ),
        })
    }
}

// Enum to store resolution of acceleration
#[derive(Debug)]
enum FrameType {
    Zero,
    One,
    Two,
}

impl FrameType {
    fn to_bytes(&self) -> usize {
        match *self {
            FrameType::Zero => 3,
            FrameType::One => 6,
            FrameType::Two => 9,
        }
    }

    fn try_from(frame: u8) -> PolarResult<FrameType> {
        match frame {
            0 => Ok(FrameType::Zero),
            1 => Ok(FrameType::One),
            2 => Ok(FrameType::Two),
            _ => Err(Error::InvalidData),
        }
    }
}

/// Struct to store acceleration from the PMD data stream
#[derive(Debug)]
pub struct Acc {
    resolution: FrameType,
    x: f32,
    y: f32,
    z: f32,
}

impl Acc {
    /// Convert data into acceleration data
    fn new(data: &Vec<u8>, resolution: FrameType) -> PolarResult<Acc> {
        if data.len() < resolution.to_bytes() {
            println!(
                "Acceleration expects {} bytes of data, got {}",
                resolution.to_bytes(),
                data.len()
            );
            return Err(Error::InvalidLength);
        }
        let frame_size = resolution.to_bytes();

        let mut x = [0u8; 4];
        x[4 - (frame_size / 3)..frame_size + 1].clone_from_slice(&data[..frame_size]);

        let mut y = [0u8; 4];
        y[4 - (frame_size / 3)..frame_size + 1].clone_from_slice(&data[frame_size..frame_size * 2]);

        let mut z = [0u8; 4];
        z[4 - (frame_size / 3)..frame_size + 1].clone_from_slice(&data[frame_size * 2..frame_size * 3]);

        Ok(Acc {
            resolution,
            x: f32::from_be_bytes(x),
            y: f32::from_be_bytes(y),
            z: f32::from_be_bytes(z),
        })
    }
}

/// Struct to store PPI from the PMD data stream
#[derive(Debug)]
pub struct Ppi {
    bpm: u8,
    peak_to_peak: u16,
    err_estimate: u16,
    flags: u8,
}

impl Ppi {
    /// Convert data into PPI data
    fn new(data: &Vec<u8>) -> PolarResult<Ppi> {
        if data.len() < 6 {
            println!("Ppi expects 6 bytes of data, got {}", data.len());
            return Err(Error::InvalidLength);
        }

        // Check for error flag
        let flags = data[5];
        if (flags & 0x1) == 0x1 {
            println!("Invalid measurement for Ppi.");
            return Err(Error::InvalidData);
        }

        Ok(Ppi {
            bpm: data[0],
            peak_to_peak: u16::from_be_bytes(
                data[1..3]
                    .try_into()
                    .expect("Error converting data to Ppi: peak to peak"),
            ),
            err_estimate: u16::from_be_bytes(
                data[3..5]
                    .try_into()
                    .expect("Error converting data to Ppi: err estimate"),
            ),
            flags,
        })
    }
}
