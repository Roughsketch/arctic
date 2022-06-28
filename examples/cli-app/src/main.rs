// example cli app using multi-threading

use arctic::PolarSensor;
use cli::{backend, input};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = input::get_id()?;
    let mut polar = PolarSensor::new(id).await?;

    // Start a channel to start/stop reading acc data
    let (tx, rx) = watch::channel(true);

    backend::init(&mut polar, rx).await?;

    input::dispatch_events(polar, tx).await?;

    Ok(())
}
