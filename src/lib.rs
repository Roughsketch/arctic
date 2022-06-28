//! # Arctic
//!
//! arctic is a library for interacting with bluetooth Polar heart rate devices.
//! It uses btleplug as the bluetooth backend which supports Windows, Mac, and Linux
//!
//! ## Usage
//!
//! Example of how to use the library to keep track of heart rate from a Polar H10
//!
//! ```rust,no_run
//! use arctic::{async_trait, Error as ArcticError, EventHandler, NotifyStream, PolarSensor, HeartRate};
//!
//! struct Handler;
//!
//! #[async_trait]
//! impl EventHandler for Handler {
//!     // Handler for heart rate events
//!     async fn heart_rate_update(&self, _ctx: &PolarSensor, heartrate: HeartRate) {
//!         println!("Heart rate: {:?}", heartrate);
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a new PolarSensor with a specific ID.
//!     // The ID is found on the device itself.
//!     let mut polar = PolarSensor::new("7B45F72B".to_string()).await.unwrap();
//!
//!     // Simple loop to continue looking for the device until it's found
//!     while !polar.is_connected().await {
//!         match polar.connect().await {
//!             Err(ArcticError::NoBleAdaptor) => {
//!                 // If there's no bluetooth adapter this library cannot work, so return.
//!                 println!("No bluetooth adapter found");
//!                 return Ok(());
//!             }
//!             Err(why) => println!("Could not connect: {:?}", why),
//!             _ => {}
//!         }
//!     }
//!
//!     // Subscribe to heart rate events
//!     if let Err(why) = polar.subscribe(NotifyStream::HeartRate).await {
//!         println!("Could not subscribe to heart rate notifications: {:?}", why)
//!     }
//!
//!     // Set the event handler to our struct defined above
//!     polar.event_handler(Handler);
//!
//!     // Run the event loop until it ends
//!     let result = polar.event_loop().await;
//!     println!("No more data: {:?}", result);
//!     Ok(())
//! }
//! ```

#![deny(missing_docs)]

pub use async_trait::async_trait;
use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::stream::StreamExt;
use std::fmt;
use std::sync::Arc;
use tokio::time::{self, Duration};
use uuid::Uuid;

mod control;
mod polar_uuid;
mod response;

pub use control::{
    ControlPoint, ControlPointCommand, ControlPointResponseCode, ControlResponse, StreamSettings,
};
use polar_uuid::{NotifyUuid, StringUuid};
pub use response::{Acc, Ecg, HeartRate, PmdData, PmdRead};

/// Error type for general errors and Ble errors from btleplug
#[derive(Debug)]
pub enum Error {
    /// Not bluetooth adapter found when trying to scan
    NoBleAdaptor,
    /// Could not create control point link
    NoControlPoint,
    /// Could not find a device when trying to connect
    NoDevice,
    /// Device is not connected, but function was called that requires it
    NotConnected,
    /// No measurement type selected
    NoDataType,
    /// Device is missing a characteristic that was used
    CharacteristicNotFound,
    /// Data packets received from device could not be parsed
    InvalidData,
    /// Not enough data was received
    InvalidLength,
    /// Command to write to PMD control point is Null
    NullCommand,
    /// Tried to create a struct using the wrong control point response
    WrongResponse,
    /// Tried to set a setting using with a `H10MeasurementType` that doesn't support that feature
    WrongType,
    /// An error occurred in the underlying BLE library
    BleError(btleplug::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Error::NoBleAdaptor => "No BLE adaptor".to_string(),
            Error::NoControlPoint => "No control point".to_string(),
            Error::NoDevice => "No device".to_string(),
            Error::NotConnected => "Not connected".to_string(),
            Error::NoDataType => "No data type".to_string(),
            Error::CharacteristicNotFound => "Characteristic not found".to_string(),
            Error::InvalidData => "Invalid data".to_string(),
            Error::InvalidLength => "Invalid length".to_string(),
            Error::NullCommand => "Null command".to_string(),
            Error::WrongResponse => "Wrong response".to_string(),
            Error::WrongType => "Wrong type".to_string(),
            Error::BleError(er) => format!("BLE error: {:?}", er),
        };
        write!(f, "Arctic Error: {}", msg)
    }
}

impl std::error::Error for Error {}

/// List of measurement types you can request
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum H10MeasurementType {
    /// Volts (V)
    Ecg,
    /// Force per unit mass (mG)
    Acc,
}

impl TryFrom<u8> for H10MeasurementType {
    type Error = ();

    fn try_from(data: u8) -> Result<H10MeasurementType, ()> {
        match data {
            0x0 => Ok(H10MeasurementType::Ecg),
            0x2 => Ok(H10MeasurementType::Acc),
            _ => Err(()),
        }
    }
}

impl H10MeasurementType {
    fn as_u8(&self) -> u8 {
        match *self {
            H10MeasurementType::Ecg => 0x0,
            H10MeasurementType::Acc => 0x2,
        }
    }

    fn as_bytes(&self) -> u8 {
        match *self {
            H10MeasurementType::Ecg => 3,
            H10MeasurementType::Acc => 6,
        }
    }
}

/// Struct that reads what features are available on your device
#[derive(Debug)]
pub struct SupportedFeatures {
    /// Electrocardiogram
    pub ecg: bool,
    /// Photoplethysmography
    pub ppg: bool,
    /// Acceleration
    pub acc: bool,
    /// Peak to peak
    pub ppi: bool,
    /// Gyroscope
    pub gyro: bool,
    /// Magnetometer
    pub mag: bool,
}

impl SupportedFeatures {
    /// Create `SupportedFeatures`
    pub fn new(mes: u8) -> SupportedFeatures {
        SupportedFeatures {
            ecg: (mes & 0b00000001) != 0,
            ppg: (mes & 0b00000010) != 0,
            acc: (mes & 0b00000100) != 0,
            ppi: (mes & 0b00001000) != 0,
            // rfu       0b00010000
            gyro: (mes & 0b00100000) != 0,
            mag: (mes & 0b01000000) != 0,
        }
    }
}

/// Trait for handling events coming from a device
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Dispatched when a battery update is received.
    ///
    /// Contains the current battery level.
    async fn battery_update(&self, _battery_level: u8) {}

    /// Dispatched when a heart rate update is received
    ///
    /// Contains information about the heart rate and R-R timing
    async fn heart_rate_update(&self, _ctx: &PolarSensor, _heartrate: HeartRate) {}

    /// Dispatched when measurement data is received over the PMD data UUID
    ///
    /// Contains data in a `PmdRead`
    async fn measurement_update(&self, _ctx: &PolarSensor, _data: PmdRead) {}

    /// Checked at start of each event loop
    ///
    /// Returns `false` if the event loop should terminate and close up
    async fn should_continue(&self) -> bool {
        true
    }
}

/// Result simplification type
pub type PolarResult<T> = std::result::Result<T, Error>;

/// A list of stream types that can be subscribed to
pub enum NotifyStream {
    /// Receive battery updates
    Battery,
    /// Receive heart rate updates
    HeartRate,
    /// Receive updates from the control points, only for use within the library
    MeasurementCP,
    /// Receive updates from the PMD data stream (acceleration or ecg)
    MeasurementData,
}

impl From<NotifyStream> for Uuid {
    fn from(item: NotifyStream) -> Self {
        NotifyUuid::from(item).into()
    }
}

/// The core Polar device structure. Keeps track of connection and event dispatching.
///
/// ## Example
///
/// Order of operations for connecting and using a `PolarSensor`
///
/// ```rust,no_run
/// // Create the initial object. The new function takes a device ID which it
/// // will use to find the device to connect to.
/// // Internally, this will set the device_id and create a
/// // a bluetooth connection manager, but it will not connect.
/// # use arctic::PolarSensor;
/// # #[tokio::main]
/// # async fn main() {
/// let mut polar = PolarSensor::new("7B45F72B".to_string()).await.unwrap();
///
/// // Do the actual connection. This will find the device and start the bluetooth connection
/// polar.connect().await.unwrap();
/// # }
/// // Can now subscribe to events, set event handler, run event_loop, etc
/// ```
pub struct PolarSensor {
    /// The device id written on the device (e.g, "8C4CAD2D")
    device_id: String,
    /// BLE connection handlers
    ble_manager: Manager,
    /// The connection to the device
    ble_device: Option<Peripheral>,
    /// Handler for event callbacks
    event_handler: Option<Arc<dyn EventHandler>>,
    /// Control point accessor
    control_point: Option<ControlPoint>,
    /// Current type of info gathered
    data_type: Option<Vec<H10MeasurementType>>,
    /// Range of 2G, 4G or 8G (only for ACC)
    range: u8,
    /// Sample rate in hz
    sample_rate: u8,
}

impl PolarSensor {
    /// Creates a new PolarSensor.
    ///
    /// # Errors
    ///
    /// Returns a [`Error::BleError`] if the bluetooth manager could not be created
    pub async fn new(device_id: String) -> PolarResult<PolarSensor> {
        let ble_manager = Manager::new().await.map_err(Error::BleError)?;

        if device_id.len() != 8 {
            return Err(Error::InvalidLength);
        }

        Ok(PolarSensor {
            device_id,
            ble_manager,
            ble_device: None,
            event_handler: None,
            control_point: None,
            data_type: None,
            range: 8,
            sample_rate: 200,
        })
    }

    /// Finds and connects to the device id associated with this device instance.
    ///
    /// # Errors
    ///
    /// Returns a [`Error::BleError`] if:
    /// - Unable to get bluetooth adapters
    /// - Unable to scan for devices
    /// - Unable to discover services for a device
    /// Also returns [`Error::NoBleAdaptor`] if there are no adapters available
    /// Can also return [`Error::NotConnected`] if no device was found
    pub async fn connect(&mut self) -> PolarResult<()> {
        // get the first bluetooth adapter
        let adapters_result = self.ble_manager.adapters().await.map_err(Error::BleError);

        if let Ok(adapters) = adapters_result {
            if adapters.is_empty() {
                return Err(Error::NoBleAdaptor);
            }

            let central = adapters.into_iter().next().unwrap();
            central
                .start_scan(ScanFilter::default())
                .await
                .map_err(Error::BleError)?;
            time::sleep(Duration::from_secs(2)).await;

            self.ble_device = self.find_device(&central).await;

            if let Some(device) = &self.ble_device {
                device.connect().await.map_err(Error::BleError)?;
                device.discover_services().await.map_err(Error::BleError)?;

                let controller = ControlPoint::new(device).await?;
                self.control_point = Some(controller);
                return Ok(());
            }

            return Err(Error::NoDevice);
        }

        Err(Error::NoBleAdaptor)
    }

    /// Subscribes to a notify event on the device. These events will be sent via the [`EventHandler`].
    ///
    /// # Errors
    ///
    /// Will return:
    /// - [`Error::NotConnected`] if the device is not currently connected
    /// - [`Error::CharacteristicNotFound`] if a given notify type is not found on the device
    /// - [`Error::BleError`] if there is an error subscribing to the event
    pub async fn subscribe(&self, stream: NotifyStream) -> PolarResult<()> {
        let device = self.device().await?;

        if let Ok(true) = device.is_connected().await {
            let characteristic = find_characteristic(device, stream.into()).await?;
            return device
                .subscribe(&characteristic)
                .await
                .map_err(Error::BleError);
        }

        Err(Error::NotConnected)
    }

    /// Unsubscribes to a notify event on your device.
    ///
    /// # Errors
    ///
    /// Will return:
    /// - [`Error::NotConnected`] if the device isn't connected
    /// - [`Error::CharacteristicNotFound`] if the specified notify type isn't found on the device
    /// - [`Error::BleError`] if there is an error subscribing to the event from within BLE
    pub async fn unsubscribe(&self, stream: NotifyStream) -> PolarResult<()> {
        let device = self.device().await?;

        if let Ok(true) = device.is_connected().await {
            let characteristic = find_characteristic(device, stream.into()).await?;

            return device
                .unsubscribe(&characteristic)
                .await
                .map_err(Error::BleError);
        }

        Err(Error::NotConnected)
    }

    /// Returns whether the device is currently connected or not
    pub async fn is_connected(&self) -> bool {
        if let Some(device) = &self.ble_device {
            if let Ok(value) = device.is_connected().await {
                return value;
            }
        }

        false
    }

    /// Returns the rssi of your device and the H10, or None if you have no device
    pub async fn rssi(&self) -> Option<i16> {
        let device = self.device().await.ok()?;

        if let Ok(Some(prop)) = device.properties().await {
            return prop.rssi;
        }

        None
    }

    /// Prints info about your H10
    /// - Model Number
    /// - Manufacturer Name
    /// - Hardware Revision
    /// - Firmware Revision
    /// - Software Revision
    /// - Serial Number
    /// - System ID
    pub async fn info(&self) {
        println!(
            "Model Number: {:?}",
            self.read_string(StringUuid::ModelNumber.into()).await
        );
        println!(
            "Manufacturer Name: {:?}",
            self.read_string(StringUuid::ManufacturerName.into()).await
        );
        println!(
            "Hardware Revision: {:?}",
            self.read_string(StringUuid::HardwareRevision.into()).await
        );
        println!(
            "Firmware Revision: {:?}",
            self.read_string(StringUuid::FirmwareRevision.into()).await
        );
        println!(
            "Software Revision: {:?}",
            self.read_string(StringUuid::SoftwareRevision.into()).await
        );
        println!(
            "Serial Number: {:?}",
            self.read_string(StringUuid::SerialNumber.into()).await
        );
        println!(
            "System ID: {:?}",
            self.read(StringUuid::SystemId.into()).await
        );
    }

    /// Prints the body location of your device
    pub async fn body_location(&self) {
        println!(
            "Body Location: {:?}",
            self.read(StringUuid::BodyLocation.into()).await
        );
    }

    /// Start measurement stream for `self.data_type`
    ///
    /// # Errors
    ///
    /// - [`Error::NoControlPoint`] if you haven't set a controller
    async fn start_measurement(&self, ty: H10MeasurementType) -> PolarResult<()> {
        let controller = self.controller().await?;
        let mut command = vec![0x02u8, ty.as_u8()];

        // Add range and resolution characteristic for acceleration only
        match ty {
            H10MeasurementType::Acc => {
                // Range
                command.push(0x02);
                command.push(0x01);
                command.push(self.range);
                command.push(0x00);

                // Sample rate
                command.push(0x00);
                command.push(0x01);
                command.push(self.sample_rate);
                command.push(0x00);

                // Resolution
                command.push(0x01);
                command.push(0x01);
                command.push(0x10);
                command.push(0x00);
            }
            H10MeasurementType::Ecg => {
                // Sample rate
                command.push(0x00);
                command.push(0x01);
                command.push(0x82);
                command.push(0x00);

                // Resolution
                command.push(0x01);
                command.push(0x01);
                command.push(0x0e);
                command.push(0x00);
            }
        }
        controller
            .send_command(self.device().await?, command)
            .await?;
        Ok(())
    }

    /// End measurement stream for `self.data_type`
    ///
    /// # Errors
    ///
    /// - [`Error::NoControlPoint`] if you haven't set a controller
    /// - [`Error::NoDataType`] if you haven't set a data type
    async fn stop_measurement(&self, data_type: H10MeasurementType) -> PolarResult<()> {
        let controller = self.controller().await?;
        controller
            .send_command(self.device().await?, [3, data_type.as_u8()].to_vec())
            .await
    }

    /// Gets the measurement settings of your H10
    pub async fn settings(&self) -> PolarResult<Vec<StreamSettings>> {
        let mut out: Vec<StreamSettings> = vec![];

        if let Some(types) = &self.data_type {
            for ty in types {
                out.push(StreamSettings::new(
                    &self
                        .get_pmd_response(ControlPointCommand::GetMeasurementSettings, *ty)
                        .await?,
                )?);
            }
        } else {
            return Err(Error::NoDataType);
        }

        Ok(out)
    }

    async fn internal_settings(&self, ty: H10MeasurementType) -> PolarResult<()> {
        let controller = self.controller().await?;
        controller
            .send_command(self.device().await?, [1, ty.as_u8()].to_vec())
            .await
    }

    /// Request the SDK features from your H10
    pub async fn features(&self) -> PolarResult<SupportedFeatures> {
        if let Ok(controller) = self.controller().await {
            if let Ok(device) = self.device().await {
                return Ok(SupportedFeatures::new(controller.read(device).await?[1]));
            }
            return Err(Error::NoDevice);
        }
        Err(Error::NoControlPoint)
    }

    async fn controller(&self) -> PolarResult<&ControlPoint> {
        if let Some(controller) = &self.control_point {
            return Ok(controller);
        }

        Err(Error::NoControlPoint)
    }

    /// Start measurement while event loop is running
    pub async fn start(&self, ty: H10MeasurementType) -> PolarResult<ControlResponse> {
        self.get_pmd_response(ControlPointCommand::RequestMeasurementStart, ty)
            .await
    }

    /// Stop measurement while event loop is running
    pub async fn stop(&self, ty: H10MeasurementType) -> PolarResult<ControlResponse> {
        self.get_pmd_response(ControlPointCommand::StopMeasurement, ty)
            .await
    }

    /// Adds this data type to read from the your H10 (if not already added)
    pub fn data_type_push(&mut self, data_type: H10MeasurementType) {
        match &mut self.data_type {
            Some(types) => {
                if types.len() != 2 && types[0] != data_type {
                    types.push(data_type);
                }
            }
            None => {
                self.data_type = Some(vec![data_type]);
            }
        };
    }

    /// Removes a data type
    pub fn data_type_pop(&mut self, data_type: H10MeasurementType) {
        if let Some(data) = &mut self.data_type {
            data.retain(|x| *x != data_type);
            if data.is_empty() {
                self.data_type = None;
            }
        }
    }

    /// Get data types
    pub fn data_type(&self) -> &Option<Vec<H10MeasurementType>> {
        &self.data_type
    }

    /// Set data range for acceleration data
    pub fn range(&mut self, range: u8) -> PolarResult<()> {
        if range == 2 || range == 4 || range == 8 {
            if let Some(ty) = &self.data_type {
                if ty.contains(&H10MeasurementType::Acc) {
                    self.range = range;
                    return Ok(());
                }

                return Err(Error::WrongType);
            }

            return Err(Error::NoDataType);
        }

        Err(Error::InvalidData)
    }

    /// Set sample rate
    pub fn sample_rate(&mut self, rate: u8) -> PolarResult<()> {
        if rate == 25 || rate == 50 || rate == 100 || rate == 200 {
            if let Some(ty) = &self.data_type {
                if ty.contains(&H10MeasurementType::Acc) {
                    self.sample_rate = rate;
                    return Ok(());
                }

                return Err(Error::WrongType);
            }

            return Err(Error::NoDataType);
        }

        Err(Error::InvalidData)
    }

    async fn device(&self) -> PolarResult<&Peripheral> {
        if let Some(device) = &self.ble_device {
            return Ok(device);
        }

        Err(Error::NoDevice)
    }

    async fn read(&self, uuid: Uuid) -> PolarResult<Vec<u8>> {
        let device = self.device().await?;

        if let Ok(char) = find_characteristic(device, uuid).await {
            return device.read(&char).await.map_err(Error::BleError);
        }

        Err(Error::CharacteristicNotFound)
    }

    async fn read_string(&self, uuid: Uuid) -> PolarResult<String> {
        let data = self.read(uuid).await?;

        let string = String::from_utf8_lossy(&data).into_owned();
        Ok(string.trim_matches(char::from(0)).to_string())
    }

    /// Sets an event handler with multiple methods for each possible event.
    pub fn event_handler<H: EventHandler + 'static>(&mut self, event_handler: H) {
        self.event_handler = Some(Arc::new(event_handler));
    }

    // Function that listens for PMD responses and returns the response and stops listening
    async fn get_pmd_response(
        &self,
        command: ControlPointCommand,
        ty: H10MeasurementType,
    ) -> PolarResult<ControlResponse> {
        // start measurement and capture response
        let mut response: PolarResult<ControlResponse> = Err(Error::NoDevice);
        if let Some(device) = &self.ble_device {
            self.subscribe(NotifyStream::MeasurementCP).await?;
            let mut notification_stream = device.notifications().await.map_err(Error::BleError)?;

            // Execute write to PMD command point
            match command {
                ControlPointCommand::Null => return Err(Error::NullCommand),
                ControlPointCommand::GetMeasurementSettings => self.internal_settings(ty).await?,
                ControlPointCommand::RequestMeasurementStart => self.start_measurement(ty).await?,
                ControlPointCommand::StopMeasurement => self.stop_measurement(ty).await?,
            };

            while let Some(data) = notification_stream.next().await {
                if data.uuid == NotifyUuid::MeasurementCP.into() {
                    response = Ok(ControlResponse::new(data.value)
                        .await
                        .expect("err value getting response"));
                    break;
                }
            }
        }
        self.unsubscribe(NotifyStream::MeasurementCP).await?;
        response
    }

    /// Run the internal event loop.
    ///
    /// This loop will receive all subscribed events and pass them on
    /// via the [`EventHandler`] trait. Make sure to connect an event handler first.
    ///
    /// # Warning
    ///
    /// If the event is started without subscribing to anything, the event loop can hang forever,
    /// and the closing condition trait function for `EventHandler` can't even close the loop.
    /// Additionally, if you're only subscribed to `MeasurementData`, you have to make sure to
    /// add a measurement type. Subscribing to `MeasurementCP` or `Battery` only also can cause
    /// issues because they will send notifications rarely.
    pub async fn event_loop(&self) -> PolarResult<()> {
        // Stop any previous measurements that might not have been stopped properly
        let _ = self
            .get_pmd_response(
                ControlPointCommand::StopMeasurement,
                H10MeasurementType::Acc,
            )
            .await?;
        let _ = self
            .get_pmd_response(
                ControlPointCommand::StopMeasurement,
                H10MeasurementType::Ecg,
            )
            .await?;

        // Start measurements
        if let Some(types) = &self.data_type {
            for ty in types {
                let _ = self
                    .get_pmd_response(ControlPointCommand::RequestMeasurementStart, *ty)
                    .await?;
            }
        }

        let eh = &self
            .event_handler
            .as_ref()
            .expect("Arctic: Event loop requires an event handler.");

        if let Some(device) = &self.ble_device {
            let mut notification_stream = device.notifications().await.map_err(Error::BleError)?;
            // Process while the BLE connection is not broken or stopped.
            while let Some(data) = notification_stream.next().await {
                if eh.should_continue().await {
                    if data.uuid == NotifyUuid::BatteryLevel.into() {
                        let battery = data.value[0];
                        eh.battery_update(battery).await;
                    } else if data.uuid == NotifyUuid::HeartMeasurement.into() {
                        let hr = HeartRate::new(data.value)?;
                        eh.heart_rate_update(self, hr).await;
                    } else if data.uuid == NotifyUuid::MeasurementData.into() {
                        if let Ok(response) = PmdRead::new(data.value) {
                            eh.measurement_update(self, response).await;
                        } else {
                            eprintln!("Invalid data received from PMD data stream.");
                        }
                    }
                } else {
                    break;
                }
            }
        }

        if let Some(types) = &self.data_type {
            for ty in types {
                self.get_pmd_response(ControlPointCommand::StopMeasurement, *ty)
                    .await?;
            }
        }

        Ok(())
    }

    async fn find_device(&self, central: &Adapter) -> Option<Peripheral> {
        for p in central.peripherals().await.unwrap() {
            if p.properties()
                .await
                .unwrap()
                .unwrap()
                .local_name
                .iter()
                .any(|name| name.starts_with("Polar") && name.ends_with(&self.device_id))
            {
                return Some(p);
            }
        }

        None
    }
}

/// Private helper to find characteristics from a uuid
async fn find_characteristic(device: &Peripheral, uuid: Uuid) -> PolarResult<Characteristic> {
    device
        .characteristics()
        .iter()
        .find(|c| c.uuid == uuid)
        .ok_or(Error::CharacteristicNotFound)
        .cloned()
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
    fn type_push() {
        let mut polar = aw!(PolarSensor::new("dummy ID".to_string())).unwrap();

        polar.data_type_push(H10MeasurementType::Acc);
        assert_eq!(polar.data_type, Some(vec![H10MeasurementType::Acc]));

        polar.data_type_push(H10MeasurementType::Ecg);
        assert_eq!(
            polar.data_type,
            Some(vec![H10MeasurementType::Acc, H10MeasurementType::Ecg])
        );

        polar.data_type_push(H10MeasurementType::Ecg);
        assert_eq!(
            polar.data_type,
            Some(vec![H10MeasurementType::Acc, H10MeasurementType::Ecg])
        );

        polar.data_type_push(H10MeasurementType::Acc);
        assert_eq!(
            polar.data_type,
            Some(vec![H10MeasurementType::Acc, H10MeasurementType::Ecg])
        );
    }

    #[test]
    fn type_pop() {
        let mut polar = aw!(PolarSensor::new("dummy ID".to_string())).unwrap();

        polar.data_type_push(H10MeasurementType::Acc);
        polar.data_type_push(H10MeasurementType::Ecg);

        polar.data_type_pop(H10MeasurementType::Acc);
        assert_eq!(polar.data_type, Some(vec![H10MeasurementType::Ecg]));

        polar.data_type_pop(H10MeasurementType::Acc);
        assert_eq!(polar.data_type, Some(vec![H10MeasurementType::Ecg]));

        polar.data_type_pop(H10MeasurementType::Ecg);
        assert_eq!(polar.data_type, None);
    }
}
