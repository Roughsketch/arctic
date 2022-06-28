// example for terminating the process using user input

use arctic::PolarSensor;
use std::env;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let device_id = back::parse_args(args.as_ref()).await?;
    let mut polar = PolarSensor::new(device_id.to_string()).await?;
    println!("Press enter to stop the program");

    // Start a channel to start/stop reading acc data
    let (tx, rx) = watch::channel(true);

    let handle = tokio::spawn(async move { back::polar_init(&mut polar, rx).await });

    let response = back::wait_for_user_input();
    tx.send(false)?;
    handle.await??;
    println!("Stopping program, user typed {:?}", response);

    Ok(())
}
