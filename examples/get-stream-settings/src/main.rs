// example for reading stream settings
use std::io::{self, Write};
use arctic::H10MeasurementType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut polar = arctic::PolarSensor::new(get_id()?)
        .await
        .expect("Invalid ID");

    println!("Attempting connection");
    while !polar.is_connected().await {
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                println!("No Bluetooth adapter found");
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

    // Set the data types of our struct
    polar.data_type_push(H10MeasurementType::Acc);
    polar.data_type_push(H10MeasurementType::Ecg);

    // Read settings from device
    println!("Settings = {:?}", polar.settings().await);

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
