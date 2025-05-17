use clap::Parser;
use rosc::OscPacket;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use yamaha_rcp_to_osc as lib;

/// Converts Yamaha RCP commands to OSC messages
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The remote console IP
    #[arg(long)]
    console_ip: String,

    /// The remote RCP port
    #[arg(long, default_value_t = 49280)]
    rcp_port: u16,

    /// The remote OSC port
    #[arg(long, default_value_t = 3999)]
    udp_osc_out_port: u16,

    /// The remote OSC address
    #[arg(long, default_value = "127.0.0.1")]
    udp_osc_out_addr: String,

    /// The local OSC port
    #[arg(long, default_value_t = 4000)]
    udp_osc_in_port: u16,

    /// The local OSC address
    #[arg(long, default_value = "0.0.0.0")]
    udp_osc_in_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // RCP (TCP) settings
    let rcp_port = args.rcp_port;
    let rcp_host = args.console_ip;

    // OSC (UDP) settings
    let osc_out_addr = format!("{}:{}", args.udp_osc_out_addr, args.udp_osc_out_port);
    let osc_in_addr = format!("{}:{}", args.udp_osc_in_addr, args.udp_osc_in_port);

    // Set up UDP sockets
    let socket_out = UdpSocket::bind("0.0.0.0:0").await?;
    let socket_in = Arc::new(UdpSocket::bind(osc_in_addr.clone()).await?);
    println!("Listening for OSC messages on: {}", osc_in_addr);
    println!("Sending OSC messages to: {}", osc_out_addr);

    // Connect to TCP RCP
    match TcpStream::connect((rcp_host.clone(), rcp_port)).await {
        Ok(stream) => {
            println!("Connected to Yamaha RCP: {}", rcp_host);
            let mut buffer = [0; 1024];
            let socket_in_clone = Arc::clone(&socket_in);
            let (mut rcp_read, rcp_write) = stream.into_split();
            let rcp_write = Arc::new(Mutex::new(rcp_write));
            let rcp_write_clone = Arc::clone(&rcp_write);

            // Spawn a task to handle incoming OSC messages
            tokio::spawn(
                async move { handle_incoming_osc(socket_in_clone, rcp_write_clone).await },
            );

            //RCP commands can sometimes be sent in bundles and should be split by newline
            let mut incomplete_line = String::new();
            loop {
                match rcp_read.read(&mut buffer).await {
                    Ok(0) => {
                        println!("Connection closed by server");
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buffer[..n]);
                        incomplete_line.push_str(&data);

                        // Process each complete line
                        while let Some(newline_pos) = incomplete_line.find('\n') {
                            let line = incomplete_line[..newline_pos].to_string();
                            incomplete_line = incomplete_line[newline_pos + 1..].to_string();
                            let parts = lib::split_respecting_quotes(line.trim());

                            if parts.is_empty() {
                                continue;
                            }

                            println!("Received RCP: {}", line.trim());

                            let osc_message = match lib::rcp_to_osc(line) {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    println!("Failed to convert RCP to OSC: {}", e);
                                    continue;
                                }
                            };

                            //This is a special work around for the Yamaha RCP
                            //The Yamaha RCP does not show all of the 'scene' data needed in sscurrent_ex
                            //So we need to send the ssinfo_ex command to get the current scene information
                            if parts[0].as_str() == "NOTIFY" && parts[1].as_str() == "sscurrent_ex"
                            {
                                let rcp_command = format!("ssinfo_ex {}\n", parts[2..].join(" "));

                                if let Err(e) = rcp_write
                                    .lock()
                                    .await
                                    .write_all(rcp_command.as_bytes())
                                    .await
                                {
                                    eprintln!("Failed to write to RCP stream: {}", e);
                                }
                            }

                            println!("Sending OSC: {}", osc_message);

                            // Convert to packet and send
                            let packet = OscPacket::Message(osc_message);
                            let encoded = rosc::encoder::encode(&packet)?;
                            socket_out.send_to(&encoded, osc_out_addr.clone()).await?;
                        }
                    }
                    Err(e) => {
                        println!("Failed to receive data: {}", e);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to connect: {}", e);
        }
    }

    Ok(())
}

async fn handle_incoming_osc(
    socket: Arc<UdpSocket>,
    stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = [0u8; 1024];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, _addr)) => {
                if let Ok((_remaining, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                    match packet {
                        OscPacket::Message(msg) => {
                            println!("Received OSC: {}", msg);
                            let rcp_command = match lib::osc_to_rcp(&msg) {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    println!("Failed to convert OSC to RCP: {}", e);
                                    continue;
                                }
                            };
                            println!("Sending RCP: {}", rcp_command);
                            if let Err(e) = stream
                                .lock()
                                .await
                                .write_all(format!("{}\n", rcp_command).as_bytes())
                                .await
                            {
                                eprintln!("Failed to write to RCP stream: {}", e);
                                continue;
                            }
                        }
                        OscPacket::Bundle(_) => {
                            println!("Received OSC bundle - not implemented");
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving OSC message: {}", e);
                break;
            }
        }
    }
    Ok(())
}
