//! Runs the engine without a window for automated tests, reading typed commands and printing events.

use opencamera_core::pairing::Device;
use opencamera_core::*;
use std::io::BufRead;
use std::sync::Arc;

struct Printer;

impl EngineEvents for Printer {
    fn state(&self, s: &EngineState) {
        println!("STATE {}", serde_json::to_string(s).unwrap());
    }
    fn stats(&self, s: &Stats) {
        println!("STATS {}", serde_json::to_string(s).unwrap());
    }
    fn preview(&self, _: Vec<u8>) {}
    fn devices(&self, d: &[Device]) {
        println!("DEVICES {}", serde_json::to_string(d).unwrap());
    }
}

fn main() -> Result<()> {
    let data_dir = std::env::args().nth(1).map(Into::into).unwrap_or_else(|| std::env::temp_dir().join("opencamera-headless"));
    let port = std::env::args().nth(2).and_then(|p| p.parse().ok()).unwrap_or(47823);
    let engine = Engine::start(
        EngineConfig {
            data_dir,
            network: NetworkConfig { port, address: None, ice_servers: vec![] },
            processing: Default::default(),
            accept_reconnects: true,
            developer: false,
        },
        Arc::new(Printer),
    )?;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let (cmd, arg) = line.split_once(' ').unwrap_or((&line, ""));
        match cmd {
            "pair" => println!("PAIRING {}", serde_json::to_string(&engine.start_pairing()?)?),
            "disconnect" => engine.disconnect(),
            "control" => engine.set_phone_settings(serde_json::from_str(arg)?),
            "quit" => break,
            _ => eprintln!("unknown command: {cmd}"),
        }
    }
    Ok(())
}
