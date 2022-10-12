// Interaction with Bluetooth

use std::sync::Mutex;
use arctic::{EventHandler, PmdRead, PolarSensor};
use tokio::sync::watch;
use std::{fs::{OpenOptions, File}, io::Write, io};
use std::time::{SystemTime, UNIX_EPOCH};

struct Handler {
    rx: watch::Receiver<bool>,
    output: Mutex<File>,
}

#[arctic::async_trait]
impl EventHandler for Handler {
    async fn measurement_update(&self, _ctx: &PolarSensor, data: PmdRead) {
        let msg = format!("{:?}\n", data.data());
        let mut output = self.output.lock().unwrap();
        let _ = output.write_all(msg.as_bytes());
        let _ = output.flush();
    }

    async fn should_continue(&self) -> bool {
        *self.rx.borrow()
    }
} 

impl Handler {
    pub async fn new(rx: watch::Receiver<bool>) -> Result<Handler, io::Error> {
        let mut output = String::from("output/hr");
        output.push_str(
            &SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Error getting time")
                .as_secs()
                .to_string(),
        );
        output.push_str(".txt");

        let open_attempt = OpenOptions::new().append(true).create(true).open(output.clone());
        let open_file = if open_attempt.is_err() {
            OpenOptions::new().append(true).create(true).open("cli-app/".to_owned() + &output).expect("Output directory not found")
        } else {
            open_attempt.unwrap()
        };

        Ok(Handler { rx, output: Mutex::new(open_file) })
    }
}

// Connect to sensor and set up data tracking
pub async fn init(polar: &mut PolarSensor, rx: watch::Receiver<bool>) -> Result<(), Box<dyn std::error::Error>> {
    while !polar.is_connected().await {
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                eprintln!("No Bluetooth adapter found");
                return Ok(());
            }
            Err(why) => eprintln!("Could not connect: {:?}", why),
            _ => {}
        }
    }

    if let Err(why) = polar.subscribe(arctic::NotifyStream::MeasurementData).await {
        eprintln!("Could not subscribe to measurment notifications: {:?}", why)
    }

    polar.event_handler(Handler::new(rx).await?);

    println!("Connected");
    Ok(())
}
