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

mod polar_uuid;
use polar_uuid::{notify_uuid, string_uuid, NotifyUuid, StringUuid};

/// Error type for general errors and Ble errors from btleplug
#[derive(Debug)]
pub enum Error {
    NoBleAdaptor,
    NotConnected,
    CharacteristicNotFound,
    /// An error occurred in the underlying BLE library
    BleError(btleplug::Error),
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
}

/// Result simplification type
pub type PolarResult<T> = std::result::Result<T, Error>;

/// A list of stream types that can be subscribed to
pub enum NotifyStream {
    Battery,
    HeartRate,
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
            central.start_scan(ScanFilter::default()).await.map_err(Error::BleError)?;
            time::sleep(Duration::from_secs(2)).await;

            self.ble_device = self.find_device(&central).await;

            if let Some(device) = &self.ble_device {
                device.connect().await.map_err(Error::BleError)?;
                device.discover_services().await.map_err(Error::BleError)?;
                return Ok(())
            }

            return Err(Error::NotConnected)
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
        if let Some(ref device) = &self.ble_device {
            if let Ok(true) = device.is_connected().await {
                let characteristic = {
                    match stream {
                        NotifyStream::Battery => {
                            self.find_characteristic(notify_uuid(NotifyUuid::BatteryLevel)).await
                        },
                        NotifyStream::HeartRate => {
                            self.find_characteristic(notify_uuid(NotifyUuid::HeartMeasurement)).await
                        }
                    }
                };

                if let Some(char) = characteristic {
                    device.subscribe(&char).await.map_err(Error::BleError)?;
                    return Ok(());
                }

                return Err(Error::CharacteristicNotFound)
            }
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

    pub async fn rssi(&self) -> Option<i16> {
        if let Some(device) = &self.ble_device {
            if let Ok(properties) = device.properties().await {
                if let Some(prop) = properties {
                    return prop.rssi;
                }
            }
        }

        None
    }

    pub async fn info(&self) {
        println!("Model Number: {:?}", self.read_string(string_uuid(StringUuid::ModelNumber)).await);
        println!("Manufacturer Name: {:?}", self.read_string(string_uuid(StringUuid::ManufacturerName)).await);
        println!("Hardware Revision: {:?}", self.read_string(string_uuid(StringUuid::HardwareRevision)).await);
        println!("Firmware Revision: {:?}", self.read_string(string_uuid(StringUuid::FirmwareRevision)).await);
        println!("Software Revision: {:?}", self.read_string(string_uuid(StringUuid::SoftwareRevision)).await);
        println!("Serial Number: {:?}", self.read_string(string_uuid(StringUuid::SerialNumber)).await);
        println!("System ID: {:?}", self.read(string_uuid(StringUuid::SystemId)).await);
    }

    pub async fn body_location(&self) {
        println!("System ID: {:?}", self.read(string_uuid(StringUuid::BodyLocation)).await);
    }

    async fn read(&self, uuid: Uuid) -> PolarResult<Vec<u8>> {
        if let Some(device) = &self.ble_device {
            if let Some(char) = self.find_characteristic(uuid).await {
                return device.read(&char).await.map_err(Error::BleError);
            }

            return Err(Error::CharacteristicNotFound);
        }

        Err(Error::NotConnected)
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
        if let Some(device) = &self.ble_device {
            let mut notification_stream = device.notifications().await.map_err(Error::BleError)?;
            // Process while the BLE connection is not broken or stopped.
            while let Some(data) = notification_stream.next().await {
                if data.uuid == notify_uuid(NotifyUuid::HeartMeasurement) {
                    println!("Data: {:?}", data.value);

                    if let Some(eh) = &self.event_handler {
                        eh.heart_rate_update(self, data.value[1].into()).await;
                    }
                    // let hrdata = process_data(data.value);
                    // let now = Utc::now();
        
                    // println!("{}, {}", now.timestamp_millis() - start, hrdata.heart_rate);
                    // println!("RR: {:?}", hrdata.rrs);
                } else if data.uuid == notify_uuid(NotifyUuid::BatteryLevel) {
                    let battery = data.value[0];
                    println!("Battery update: {}", battery);

                    if let Some(eh) = &self.event_handler {
                        eh.battery_update(battery).await;
                    }
                }
            }
        }

        Ok(())
    }

    async fn find_characteristic(&self, uuid: Uuid) -> Option<Characteristic> {
        if let Some(device) = &self.ble_device {
            let characteristics = device.characteristics(); 
            if let Some(characteristic) = characteristics.iter().find(|c| c.uuid == uuid) {
                Some(characteristic.clone())
            } else {
                None
            }
        } else {
            None
        }
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
