// Get user input

use std::sync::Arc;
use arctic::{H10MeasurementType, PolarSensor};
use std::{io, io::Write};
use tokio::sync::watch;

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

// Handle events sent by the usertokio::spawn(async {ctx.event_loop()})
pub async fn dispatch_events(
    mut ctx: PolarSensor,
    tx: watch::Sender<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Type `help` for a list of commands.");
    loop {
        let mut cmd = String::new();
        print!("(arctic) ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut cmd)?;

        if cmd.ends_with('\n') {
            cmd.pop();
            if cmd.ends_with('\r') {
                cmd.pop();
            }
        }
        let args: Vec<&str> = cmd.split(' ').collect();
        match args[0] {
            "quit" => {
                return Ok(());
            }
            "add" => {
                if args.len() != 2 {
                    eprintln!(
                        "Invalid command, use `add <TYPE>` or type `help` for more information"
                    );
                    continue;
                }
                match args[1] {
                    "ecg" => ctx.data_type_push(H10MeasurementType::Ecg),
                    "acc" => ctx.data_type_push(H10MeasurementType::Acc),
                    _ => eprintln!("Invalid type, options are `ecg` and `acc`"),
                }
            }
            "remove" => {
                if args.len() != 2 {
                    eprintln!(
                        "Invalid command, use `remove <TYPE>` or type `help` for more information"
                    );
                    continue;
                }
                match args[1] {
                    "ecg" => ctx.data_type_pop(H10MeasurementType::Ecg),
                    "acc" => ctx.data_type_pop(H10MeasurementType::Acc),
                    _ => eprintln!("Invalid type, options are `ecg` and `acc`"),
                }
            }
            "types" => match ctx.data_type() {
                Some(x) => println!("types: {:?}", x),
                None => println!("no types specified"),
            },
            "run" => {
                if let Some(data) = &ctx.data_type() {
                    if !data.is_empty() {
                        break;
                    }
                }
                eprintln!("A data type needs to be added before starting the event loop.");
            },
            "features" => println!("features: {:?}", ctx.features().await?),
            "help" => start_help(),
            "clear" => clearscreen::clear().expect("Error clearing the screen"),
            _ => eprintln!("Invalid command. Type `help` for a list of commands"),
        }
    }

    println!("Now in running mode, type `help` for a list of commands");
    let ctx = Arc::new(ctx);
    let ctx_1 = Arc::clone(&ctx);
    let handle = tokio::spawn(async move { ctx_1.event_loop().await });

    loop {
        let mut cmd = String::new();
        print!("(arctic) ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut cmd)?;

        if cmd.ends_with('\n') {
            cmd.pop();
            if cmd.ends_with('\r') {
                cmd.pop();
            }
        }
        let args: Vec<&str> = cmd.split(' ').collect();
        match args[0] {
            "quit" | "stop" => {
                tx.send(false)?;
                let ret = handle.await?;
                if ret.is_ok() {
                    return Ok(());
                } else {
                    return Err(Box::new(ret.unwrap_err()));
                }
            }
            "help" => run_help(),
            "add" => {
                if args.len() != 2 {
                    eprintln!("Invalid command, use `add <TYPE>` or type `help` for more information");
                    continue;
                }
                match args[1] {
                    "ecg" => println!("response: {:?}", ctx.start(H10MeasurementType::Ecg).await),
                    "acc" => println!("response: {:?}", ctx.start(H10MeasurementType::Acc).await),
                    _ => eprintln!("Invalid type, options are `ecg` and `acc`"),
                }
            },
            "remove" => {
                if args.len() != 2 {
                    eprintln!("Invalid command, use `remove <TYPE>` or type `help` for more information");
                    continue;
                }
                match args[1] {
                    "ecg" => println!("response: {:?}", ctx.stop(H10MeasurementType::Ecg).await),
                    "acc" => println!("response: {:?}", ctx.stop(H10MeasurementType::Acc).await),
                    _ => eprintln!("Invalid type, options are `ecg` and `acc`"),
                }
            },
            "types" => match ctx.data_type() {
                Some(x) => println!("types: {:?}", x),
                None => println!("no types specified"),
            },
            "features" => println!("features: {:?}", ctx.features().await?),
            "clear" => clearscreen::clear().expect("Error clearing the screen"), // cross platform clear
            _ => eprintln!("Invalid command. Type `help` for a list of commands"),
        }
    }
}

fn start_help() {
    println!("    quit               Stop the program and all measurements");
    println!("    help               Get a list of commands");
    println!(
        "    add <TYPE>         Add a type to measure from the Polar H10"
    );
    println!("    remove <TYPE>      Remove a type from the Polar H10");
    println!("    types              Prints any types specified currently");
    println!("    run                Starts reading data from the device");
    println!("    features           Get the features of the device");
    println!("    clear              Clears the screen for those of you who are annoyed with the clutter");
}

fn run_help() {
    println!("    quit or stop       Stop the program and all measurements");
    println!("    help               Get a list of commands");
    println!(
        "    add <TYPE>         Start receiving measurements from TYPE"
    );
    println!("    remove <TYPE>      Stop receiving measurements from Type");
    println!("    types              Prints any types specified before running");
    println!("    features           Get the features of the device");
    println!("    clear              Clears the screen for those of you who are annoyed with the clutter");
}
