// Interaction with bluetooth

use arctic::{Error, EventHandler, H10MeasurementType, PmdRead, PolarResult, PolarSensor};
use std::io;
use tokio::sync::watch;

struct Handler {
    rx: watch::Receiver<bool>,
}

#[arctic::async_trait]
impl EventHandler for Handler {
    async fn measurement_update(&self, _ctx: &PolarSensor, data: PmdRead) {
        println!("{:?}", data.data());
    }

    async fn should_continue(&self) -> bool {
        *self.rx.borrow()
    }
}

// Read arguments into PolarSensor
pub async fn parse_args(args: &[String]) -> PolarResult<&str> {
    if args.len() != 2 {
        eprintln!("No device ID found.");
        return Err(Error::InvalidLength);
    }

    Ok(&args[1])
}

// Connect to sensor and set up data tracking
pub async fn polar_init(polar: &mut PolarSensor, rx: watch::Receiver<bool>) -> PolarResult<()> {
    println!("Attempting connection");
    while !polar.is_connected().await {
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                eprintln!("No bluetooth adapter found");
                return Ok(());
            }
            Err(why) => eprintln!("Could not connect: {:?}", why),
            _ => {}
        }
    }
    println!("Connected");

    if let Err(why) = polar.subscribe(arctic::NotifyStream::MeasurementData).await {
        eprintln!("Could not subscribe to measurment notifications: {:?}", why)
    }

    polar.data_type_push(H10MeasurementType::Acc);
    polar.range(8)?;
    polar.event_handler(Handler { rx });

    polar.event_loop().await
}

// Stop main thread while waiting for input
pub fn wait_for_user_input() -> Result<String, io::Error> {
    let mut msg = String::new();
    io::stdin().read_line(&mut msg)?;

    Ok(msg)
}
