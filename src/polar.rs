use async_trait::async_trait;
use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral};
use tokio::time::{self, Duration};
use uuid::Uuid;

use std::sync::Arc;
use futures::stream::StreamExt;

const BATTERY_LEVEL_UUID: Uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
const HR_MEASUREMENT_UUID: Uuid = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);

#[derive(Debug)]
pub enum Error {
    NoBleAdaptor,
    NotConnected,
    CharacteristicNotFound,
    /// An error occurred in the underlying BLE library
    BleError(btleplug::Error),
}

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn battery_update(&self, _battery_level: u8) {}
    async fn heartrate_update(&self, _ctx: &PolarSensor, _heartrate: u16) {}
}

pub type PolarResult<T> = std::result::Result<T, Error>;

pub enum NotifyStream {
    Battery,
    HeartRate,
}

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
    pub async fn new(device_id: String) -> PolarResult<PolarSensor> {
        let ble_manager = Manager::new().await.map_err(Error::BleError)?;

        Ok(PolarSensor {
            device_id,
            ble_manager,
            ble_device: None,
            event_handler: None,
        })
    }

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

    pub async fn subscribe(&self, stream: NotifyStream) -> PolarResult<()> {
        if let Some(ref device) = &self.ble_device {
            if let Ok(true) = device.is_connected().await {
                let characteristic = {
                    match stream {
                        NotifyStream::Battery => self.find_characteristic(BATTERY_LEVEL_UUID).await,
                        NotifyStream::HeartRate => self.find_characteristic(HR_MEASUREMENT_UUID).await,
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

    /// Sets an event handler with multiple methods for each possible event.
    pub fn event_handler<H: EventHandler + 'static>(&mut self, event_handler: H) {
        self.event_handler = Some(Arc::new(event_handler));
    }

    pub async fn event_loop(&self) -> PolarResult<()> {
        // let start = Utc::now().timestamp_millis();
        if let Some(device) = &self.ble_device {
            let mut notification_stream = device.notifications().await.map_err(Error::BleError)?;
            // Process while the BLE connection is not broken or stopped.
            while let Some(data) = notification_stream.next().await {
                if data.uuid == HR_MEASUREMENT_UUID {
                    println!("Data: {:?}", data.value);

                    if let Some(eh) = &self.event_handler {
                        eh.heartrate_update(self, data.value[1].into()).await;
                    }
                    // let hrdata = process_data(data.value);
                    // let now = Utc::now();
        
                    // println!("{}, {}", now.timestamp_millis() - start, hrdata.heart_rate);
                    // println!("RR: {:?}", hrdata.rrs);
                } else if data.uuid == BATTERY_LEVEL_UUID {
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