/// arctic is a library for interacting with bluetooth Polar heart rate devices
/// It uses btleplug as the bluetooth backend which supports Windows, Mac, and Linux
///
/// ## Usage
///
/// Example of how to use the library to keep track of heart rate from a Polar H10
///
/// ```rust,no_run
/// use arctic::{async_trait, Error as ArcticError, EventHandler, NotifyStream, PolarSensor};
///
/// struct Handler;
///
/// #[async_trait]
/// impl EventHandler for Handler {
///     // Handler for heart rate events
///     async fn heart_rate_update(&self, _ctx: &PolarSensor, heartrate: u16) {
///         println!("Heart rate: {}", heartrate);
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Create a new PolarSensor with a specific ID.
///     // The ID is found on the device itself.
///     let mut polar = PolarSensor::new("7B45F72B".to_string()).await.unwrap();
///
///     // Simple loop to continue looking for the device until it's found
///     while !polar.is_connected().await {
///         match polar.connect().await {
///             Err(ArcticError::NoBleAdaptor) => {
///                 // If there's no bluetooth adapter this library cannot work, so return.
///                 println!("No bluetooth adapter found");
///                 return Ok(());
///             }
///             Err(why) => println!("Could not connect: {:?}", why),
///             _ => {}
///         }
///     }
///
///     // Subscribe to heart rate events
///     if let Err(why) = polar.subscribe(NotifyStream::HeartRate).await {
///         println!("Could not subscribe to heart rate notifications: {:?}", why)
///     }
///
///     // Set the event handler to our struct defined above
///     polar.event_handler(Handler);
///
///     // Run the event loop until it ends
///     let result = polar.event_loop().await;
///     println!("No more data: {:?}", result);
///     Ok(())
/// }
/// ```
pub use async_trait::async_trait;
use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::stream::StreamExt;
use tokio::time::{self, Duration};
use uuid::Uuid;

use std::sync::Arc;

mod control;
mod polar_uuid;

use control::{ControlPoint, PmdRead};
use polar_uuid::{NotifyUuid, StringUuid};

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
    /// An error occurred in the underlying BLE library
    BleError(btleplug::Error),
}

/// List of measurement types you can request (one at a time)
#[derive(Debug)]
pub enum H10MeasurementType {
    /// Volts (V)
    Ecg,
    ///
    Ppg,
    /// Force per unit mass (g)
    Acc,
    /// Seconds (s)
    Ppi,
}

impl TryFrom<u8> for H10MeasurementType {
    type Error = ();

    fn try_from(data: u8) -> Result<H10MeasurementType, ()> {
        match data {
            0x0 => Ok(H10MeasurementType::Ecg),
            0x1 => Ok(H10MeasurementType::Ppg),
            0x2 => Ok(H10MeasurementType::Acc),
            0x3 => Ok(H10MeasurementType::Ppi),
            _ => Err(()),
        }
    }
}

impl H10MeasurementType {
    fn to_u8(&self) -> u8 {
        match *self {
            H10MeasurementType::Ecg => 0x0,
            H10MeasurementType::Ppg => 0x1,
            H10MeasurementType::Acc => 0x2,
            H10MeasurementType::Ppi => 0x3,
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
    async fn heart_rate_update(&self, _ctx: &PolarSensor, _heartrate: u16) {}

    async fn measurement_data(&self, _ctx: &PolarSensor, _data: Vec<u8>) {}
}

/// Result simplification type
pub type PolarResult<T> = std::result::Result<T, Error>;

/// A list of stream types that can be subscribed to
pub enum NotifyStream {
    Battery,
    HeartRate,
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
/// let mut polar = PolarSensor::new("7B45F72B".to_string()).await.unwrap();
///
/// // Do the actual connection. This will find the device and start the bluetooth connection
/// polar.connect().await.unwrap();
///
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
    data_type: Option<H10MeasurementType>,
}

impl PolarSensor {
    /// Creates a new PolarSensor.
    ///
    /// # Errors
    ///
    /// Returns a [`Error::BleError`] if the bluetooth manager could not be created
    pub async fn new(device_id: String) -> PolarResult<PolarSensor> {
        let ble_manager = Manager::new().await.map_err(Error::BleError)?;

        Ok(PolarSensor {
            device_id,
            ble_manager,
            ble_device: None,
            event_handler: None,
            control_point: None,
            data_type: None,
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
    /// Also returns [`Error::NoBleAdapter`] if there are no adapters available
    /// Can also return [`Error::NotConnected`] if no device was found
    pub async fn connect(&mut self) -> PolarResult<()> {
        // get the first bluetooth adapter
        let adapters_result = self.ble_manager.adapters().await.map_err(Error::BleError);

        if let Ok(adapters) = adapters_result {
            if adapters.is_empty() {
                return Err(Error::NoBleAdaptor);
            }

            let central = adapters.into_iter().nth(0).unwrap();
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
    /// - [`Error::BlueError`] if there is an error subscribing to the event
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

        if let Ok(properties) = device.properties().await {
            if let Some(prop) = properties {
                return prop.rssi;
            }
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

    /// Gets the measurement settings of your H10
    pub async fn settings(&self) -> PolarResult<()> {
        let controller = self.controller().await?;
        controller
            .send_command(self.device().await?, [1, 0].to_vec())
            .await
    }

    /// Start measurement stream for `self.data_type`
    ///
    /// # Errors
    ///
    /// - [`NoControlPoint`] if you haven't set a controller
    /// - [`NoDataType`] if you haven't set a data type
    pub async fn start_measurement(&self) -> PolarResult<()> {
        let controller = self.controller().await?;
        if let Some(data_type) = &self.data_type {
            return controller
                .send_command(
                    self.device().await?, // example using acceleration
                    [
                        0x02,
                        data_type.to_u8(),
                        0x02,
                        0x01,
                        0x08,
                        0x00,
                        0x00,
                        0x01,
                        0xC8,
                        0x00,
                        0x01,
                        0x01,
                        0x10,
                        0x00,
                    ]
                    .to_vec(),
                )
                .await;
        }

        Err(Error::NoDataType)
    }

    /// End measurement stream for `self.data_type`
    ///
    /// # Errors
    ///
    /// - [`NoControlPoint`] if you haven't set a controller
    /// - [`NoDataType`] if you haven't set a data type
    pub async fn stop_measurement(&self) -> PolarResult<()> {
        let controller = self.controller().await?;
        if let Some(data_type) = &self.data_type {
            return controller
                .send_command(self.device().await?, [3, data_type.to_u8()].to_vec())
                .await;
        }

        Err(Error::NoDataType)
    }

    pub async fn full_settings(&self) -> PolarResult<()> {
        let controller = self.controller().await?;
        controller
            .send_command(self.device().await?, [4, 0].to_vec())
            .await // 4??... 1 is used to request settings, and option two is for what type of data you want
    }

    async fn controller(&self) -> PolarResult<&ControlPoint> {
        if let Some(controller) = &self.control_point {
            return Ok(controller);
        }

        Err(Error::NoControlPoint)
    }

    /// Sets the data type to read from the your H10
    pub fn data_type(&mut self, data_type: H10MeasurementType) {
        self.data_type = Some(data_type);
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

        return Err(Error::CharacteristicNotFound);
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

    /// Run the internal event loop.
    ///
    /// This loop will receive all subscribed events and pass them on
    /// via the [`EventHandler`] trait. Make sure to connect an event handler first.
    pub async fn event_loop(&self) -> PolarResult<()> {
        // let start = Utc::now().timestamp_millis();
        if let Some(_) = self.data_type {
            self.start_measurement().await?;
        }

        if let Some(device) = &self.ble_device {
            let mut notification_stream = device.notifications().await.map_err(Error::BleError)?;
            // Process while the BLE connection is not broken or stopped.
            while let Some(data) = notification_stream.next().await {
                if data.uuid == NotifyUuid::BatteryLevel.into() {
                    let battery = data.value[0];
                    // println!("Battery update: {}", battery);

                    if let Some(eh) = &self.event_handler {
                        eh.battery_update(battery).await;
                    }
                } else if data.uuid == NotifyUuid::HeartMeasurement.into() {
                    // println!("Data: {:?}", data.value);

                    if let Some(eh) = &self.event_handler {
                        eh.heart_rate_update(self, data.value[1].into()).await;
                    }
                    // let hrdata = process_data(data.value);
                    // let now = Utc::now();

                    // println!("{}, {}", now.timestamp_millis() - start, hrdata.heart_rate);
                    // println!("RR: {:?}", hrdata.rrs);
                } else if data.uuid == NotifyUuid::MeasurementData.into() {
                    println!("Data: {:?}", &data.value);
                    if let Ok(response) = PmdRead::new(data.value) {
                        println!("Data: {:?}", response);
                    } else {
                        println!("Invalid data received from PMD data stream");
                    }
                }
            }
        }

        if let Some(_) = self.data_type {
            self.stop_measurement().await?;
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
