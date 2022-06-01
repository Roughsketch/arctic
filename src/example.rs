// example

use arctic::H10MeasurementType;

struct Handler;

#[arctic::async_trait]
impl arctic::EventHandler for Handler {
    async fn battery_update(&self, battery_level: u8) {
        println!("Battery: {}", battery_level);
    }

    async fn heart_rate_update(&self, _ctx: &arctic::PolarSensor, heartrate: u16) {
        println!("Heart rate: {}", heartrate);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut polar = arctic::PolarSensor::new("9F8F7521".to_string())
        .await
        .unwrap();

    while !polar.is_connected().await {
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                println!("No bluetooth adapter found");
                return Ok(());
            }
            Err(why) => println!("Could not connect: {:?}", why),
            _ => {}
        }
    }

    if let Err(why) = polar.subscribe(arctic::NotifyStream::HeartRate).await {
        println!("Could not subscirbe to heart rate notifications: {:?}", why)
    }

    if let Err(why) = polar.subscribe(arctic::NotifyStream::MeasurementData).await {
        println!(
            "Could not subscribe to measurement data notifications: {:?}",
            why
        )
    }

    println!("settings: {:?}", polar.settings().await);
    polar.data_type(H10MeasurementType::Acc);
    polar.event_handler(Handler);
    let result = polar.event_loop().await;

    println!("No more data: {:?}", result);

    Ok(())
}
