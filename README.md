# Arctic

Work in progress Rust library for handling Polar bluetooth heart rate monitors. Currently only targetting support for H10 due to lack of other devices.

# Example

```rust,ignore
use arctic::{async_trait};

struct Handler;

#[async_trait]
impl arctic::EventHandler for Handler {
    async fn battery_update(&self, battery_level: u8) {
        println!("Battery: {}", battery_level);
    }

    async fn heartrate_update(&self, _ctx: &arctic::PolarSensor, heartrate: u16) {
        println!("Heart rate: {}", heartrate);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut polar = arctic::PolarSensor::new("7B45F72B".to_string()).await.unwrap();
    
    while !polar.is_connected().await {
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                println!("No bluetooth adapter found");
                return Ok(());
            },
            Err(why) => println!("Could not connect: {:?}", why),
            _ => {},
        }
    }

    if let Err(why) = polar.subscribe(arctic::NotifyStream::Battery).await {
        println!("Could not subscribe to battery notifications: {:?}", why)
    }

    if let Err(why) = polar.subscribe(arctic::NotifyStream::HeartRate).await {
        println!("Could not subscribe to heart rate notifications: {:?}", why)
    }


    polar.event_handler(Handler);
    let result = polar.event_loop().await;

    println!("No more data: {:?}", result);
    
    Ok(())

}
```