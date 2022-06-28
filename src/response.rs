//! # Response
//!
//! Response contains types related to PMD data respones. Structures to interpret this data are found here
//!

use crate::{Error, H10MeasurementType, PolarResult};

// Helper function to convert variant length byte arrays to i32 numbers
fn bytes_to_data(data: &[u8], len: usize) -> i32 {
    if len == 3 {
        let mut buf = [0u8; 4];
        buf[..len].copy_from_slice(&data[..len]);

        // Check if most significant byte is negative and retain that negative
        if (data[len - 1] & 0x80) > 0 {
            for i in buf[len..].iter_mut() {
                *i = 0xff;
            }
        }

        i32::from_le_bytes(buf)
    } else if len == 2 {
        let mut buf = [0u8; 2];
        buf[..len].copy_from_slice(&data[..len]);

        if (data[len - 1] & 0x80) > 0 {
            for i in buf[len..].iter_mut() {
                *i = 0xff;
            }
        }

        let small = i16::from_le_bytes(buf);
        i32::from(small)
    } else {
        let mut buf = [0u8; 1];
        buf[..len].copy_from_slice(&data[..len]);

        let small = i8::from_le_bytes(buf);
        i32::from(small)
    }
}

/// Struct for reveiving measurement type data on PMD data
#[derive(Debug)]
pub struct PmdRead {
    data_type: H10MeasurementType,
    time_stamp: u64,
    data: Vec<PmdData>,
}

impl PmdRead {
    /// Create new `PmdRead`
    pub fn new(data_stream: Vec<u8>) -> PolarResult<PmdRead> {
        let data_type = H10MeasurementType::try_from(data_stream[0]);
        if let Err(_e) = data_type {
            return Err(Error::InvalidData);
        }
        let data_type = data_type.unwrap();
        let time_stamp = u64::from_le_bytes(
            data_stream[1..9]
                .try_into()
                .expect("Timestamp slice could not be converted to u64"),
        );

        // Read all samples from data stream
        let frame_length = data_type.as_bytes() as usize;
        let samples = data_stream[10..].len() / frame_length;
        let mut data: Vec<PmdData> = Vec::new();
        let mut current_pos = 10;

        for _ in 0..samples {
            data.push(match data_type {
                H10MeasurementType::Ecg => PmdData::Ecg(Ecg::new(
                    &data_stream[current_pos..current_pos + frame_length].to_vec(),
                )?),
                H10MeasurementType::Acc => PmdData::Acc(Acc::new(
                    &data_stream[current_pos..current_pos + frame_length].to_vec(),
                )?),
            });
            current_pos += frame_length;
        }

        Ok(PmdRead {
            data_type,
            time_stamp,
            data,
        })
    }

    /// Return data type of this data
    pub fn data_type(&self) -> &H10MeasurementType {
        &self.data_type
    }

    /// Return timestamp of this data
    pub fn time_stamp(&self) -> u64 {
        self.time_stamp
    }

    /// Consumes self and returns all data
    pub fn data(self) -> Vec<PmdData> {
        self.data
    }
}

/// Enum to store which kind of data was received
#[derive(Debug)]
pub enum PmdData {
    /// Electrocardiagram
    Ecg(Ecg),
    /// Acceleration
    Acc(Acc),
}

/// Struct to store ECG from the PMD data stream
#[derive(Debug)]
pub struct Ecg {
    val: i32,
}

impl Ecg {
    /// Convert data into ECG data
    fn new(data: &Vec<u8>) -> PolarResult<Ecg> {
        if data.len() < 3 {
            eprintln!("ECG expects 3 bytes of data, got {}.", data.len());
            return Err(Error::InvalidLength);
        }
        let mut mag = [0u8; 4];
        mag[1..4].clone_from_slice(&data[..3]);

        let val = bytes_to_data(&data[..3], 3);

        Ok(Ecg { val })
    }

    /// Return ECG value (in µV)
    pub fn val(&self) -> &i32 {
        &self.val
    }
}

/// Struct to store acceleration from the PMD data stream
#[derive(Debug)]
pub struct Acc {
    x: i32,
    y: i32,
    z: i32,
}

impl Acc {
    /// Convert data into acceleration data
    fn new(data: &Vec<u8>) -> PolarResult<Acc> {
        if data.len() < 2 {
            eprintln!("Acceleration expects 2 bytes of data, got {}", data.len());
            return Err(Error::InvalidLength);
        }
        let frame_size = 6;

        Ok(Acc {
            x: bytes_to_data(&data[..frame_size / 3], frame_size / 3),
            y: bytes_to_data(&data[frame_size / 3..(frame_size / 3) * 2], frame_size / 3),
            z: bytes_to_data(
                &data[(frame_size / 3) * 2..(frame_size / 3) * 3],
                frame_size / 3,
            ),
        })
    }

    /// Return data as a tuple (in mG)
    pub fn data(&self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }
}

/// Structure to contain HR data and RR interval
#[derive(Debug)]
pub struct HeartRate {
    bpm: u8,
    rr: Option<Vec<u16>>,
}

impl HeartRate {
    /// Create new instance of HR data
    pub fn new(data: Vec<u8>) -> PolarResult<HeartRate> {
        if data.len() < 2 {
            eprintln!(
                "Heart rate expects atleast 2 bytes of data, got {}",
                data.len()
            );
            return Err(Error::InvalidLength);
        }
        let flags = data[0];
        let samples = if flags & 0b00010000 == 16 {
            (data.len() - 2) / 2
        } else {
            0
        };

        let bpm = data[1];
        let mut rr_samp = vec![];

        for i in 0..samples {
            rr_samp.push(((bytes_to_data(&data[i * 2 + 2..i * 2 + 4], 2) as u32 * 128) / 125) as u16); // rr values are stored as 1024ths of a second, convert to ms
        }

        let rr = if !rr_samp.is_empty() {
            Some(rr_samp)
        } else {
            None
        };

        Ok(HeartRate { bpm, rr })
    }

    /// Get BPM of heartrate measurement
    pub fn bpm(&self) -> &u8 {
        &self.bpm
    }

    /// Get RR interval as a tuple
    pub fn rr(&self) -> &Option<Vec<u16>> {
        &self.rr
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // Test PmdRead constructor with ACC data
    #[test]
    fn pmd_read_acc_new() {
        let response = PmdRead::new(vec![
            0x02, 0xea, 0x54, 0xa2, 0x42, 0x8b, 0x45, 0x52, 0x08, 0x01, 0x45, 0xff, 0xe4, 0xff,
            0xb5, 0x03, 0x45, 0xff, 0xe4, 0xff, 0xb8, 03,
        ])
        .unwrap();

        assert_eq!(*response.data_type(), H10MeasurementType::Acc);
        assert_eq!(response.time_stamp(), 599618164814402794u64);
        let the_data = response.data();
        match &the_data[0] {
            PmdData::Acc(thing) => {
                let (x, y, z) = thing.data();
                assert_eq!(x, -187);
                assert_eq!(y, -28);
                assert_eq!(z, 949);
            }
            _ => panic!("Instantiated object of wrong type, expected Acc"),
        }
    }

    #[test]
    fn pmd_read_ecg_new() {
        let response = PmdRead::new(vec![
            0x00, 0xea, 0x54, 0xa2, 0x42, 0x8b, 0x45, 0x52, 0x08, 0x00, 0xff, 0xff, 0xff,
        ])
        .unwrap();

        assert_eq!(*response.data_type(), H10MeasurementType::Ecg);
        assert_eq!(response.time_stamp(), 599618164814402794u64);
        let the_data = response.data();
        match &the_data[0] {
            PmdData::Ecg(thing) => assert_eq!(*thing.val(), -1),
            _ => panic!("Instantiated object of wrong type, expected Ecg"),
        }
    }

    // Test that the converter for acceleration is working properly
    #[test]
    fn convert_i24_to_i32() {
        let data = [0xff, 0xff, 0xff];

        assert_eq!(-1, bytes_to_data(&data[..], 3));

        let data = [0x00, 0x00, 0x10];

        assert_eq!(1_048_576, bytes_to_data(&data[..], 3));
    }

    #[test]
    fn convert_i16_to_i32() {
        let data = [0xff, 0xff];

        assert_eq!(-1, bytes_to_data(&data[..], 2));

        let data = [0x00, 0x10];

        assert_eq!(4096, bytes_to_data(&data[..], 2));
    }

    #[test]
    fn convert_i8_to_i32() {
        let data = [0xff];

        assert_eq!(-1, bytes_to_data(&data[..], 1));

        let data = [0x10];

        assert_eq!(16, bytes_to_data(&data[..], 1));
    }

    // Check that acceleration is working properly
    #[test]
    fn hr_new() {
        let hr = HeartRate::new(vec![16, 60, 55, 4, 7, 3]).unwrap();

        assert_eq!(*hr.bpm(), 60);
        assert_eq!(*hr.rr(), Some(vec![1104, 793]));
    }
}
