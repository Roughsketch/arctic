// example for reading heart rate data
use std::io::{self, Write};

struct Handler;

#[arctic::async_trait]
impl arctic::EventHandler for Handler {
    async fn heart_rate_update(&self, _ctx: &arctic::PolarSensor, heartrate: arctic::HeartRate) {
        println!("Heart rate: {:?}", heartrate);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut polar = arctic::PolarSensor::new(get_id()?)
        .await
        .expect("Invalid ID");

    println!("Attempting connection");
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
    println!("Connected");

    if let Err(why) = polar.subscribe(arctic::NotifyStream::HeartRate).await {
        println!("Could not subscirbe to heart rate notifications: {:?}", why)
    }

    polar.event_handler(Handler);

    let result = polar.event_loop().await;

    println!("No more data: {:?}", result);

    Ok(())
}

pub fn get_id() -> Result<String, Box<dyn std::error::Error>> {
    let mut id = String::new();

    print!("Input device ID: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut id)?;

    if id.ends_with('\n') {
        id.pop();
        if id.ends_with('\r') {
            id.pop();
        }
    }

    Ok(id)
}
