use rosc::{OscMessage, OscPacket, OscType};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The remote console IP
    #[arg(short, long)]
    console_ip: String,

    /// The remote RCP port
    #[arg(short, long, default_value_t = 49280)]
    rcp_port: u16,
}

fn rcp_to_osc_arg(arg: &str) -> OscType {
    if let Ok(i) = arg.parse::<i32>() {
        OscType::Int(i)
    } else if let Ok(f) = arg.parse::<f32>() {
        OscType::Float(f)
    } else {
        OscType::String(arg.to_string())
    }
}

fn osc_to_rcp_arg(arg: &OscType) -> String {
    match arg {
        OscType::Int(i) => i.to_string(),
        OscType::Float(f) => f.to_string(),
        OscType::String(s) => s.clone(),
        _ => String::from("0"), // Default value for unsupported types
    }
}

//https://github.com/bitfocus/companion-module-yamaha-rcp/blob/b0dfb601d142f3aa14120aad7561f8691641834e/paramFuncs.js#L150
//https://github.com/search?q=repo%3Abitfocus%2Fcompanion-module-yamaha-rcp+sscurrent_ex&type=code
//https://discourse.checkcheckonetwo.com/t/ql-series-scp-commands/2266/21
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // RCP (TCP) settings
    let rcp_port = args.rcp_port;
    let rcp_host = args.console_ip;

    // OSC (UDP) settings
    let osc_out_addr = "127.0.0.1:3999";
    let osc_in_addr = "0.0.0.0:4000"; // Listen for incoming OSC on port 8001

    // Set up UDP sockets
    let socket_out = UdpSocket::bind("0.0.0.0:0").await?;
    let socket_in = Arc::new(UdpSocket::bind(osc_in_addr).await?);
    println!("Listening for OSC messages on: {}", osc_in_addr);
    println!("Sending OSC messages to: {}", osc_out_addr);

    // Connect TCP
    match TcpStream::connect((rcp_host.clone(), rcp_port)).await {
        Ok(stream) => {
            println!("Connected to Yamaha RCP: {}", rcp_host);
            let mut buffer = [0; 1024];
            let socket_in_clone = Arc::clone(&socket_in);
            let (tcp_read, tcp_write) = stream.into_split();

            // Spawn a task to handle incoming OSC messages
            tokio::spawn(async move { handle_incoming_osc(socket_in_clone, tcp_write).await });

            // Use tcp_read for the main loop
            let mut stream = tcp_read;

            let mut incomplete_line = String::new();
            loop {
                match stream.read(&mut buffer).await {
                    Ok(n) if n == 0 => {
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

                            // Process the complete line
                            let parts: Vec<&str> = line.trim().split_whitespace().collect();

                            if parts.is_empty() {
                                continue;
                            }

                            println!("Received RCP: {}", line.trim());

                            match parts[0] {
                                "NOTIFY" | "OK" => {
                                    // Create OSC message
                                    let osc_addr_pattern =
                                        format!("/{}/{}/{}", parts[0], parts[1], parts[2]);

                                    let args: Vec<OscType> =
                                        parts[3..].iter().map(|part| rcp_to_osc_arg(part)).collect();

                                    let msg = OscMessage {
                                        addr: osc_addr_pattern.clone(),
                                        args,
                                    };

                                    println!("Sent OSC: {}", msg);

                                    // Convert to packet and send
                                    let packet = OscPacket::Message(msg);
                                    let encoded = rosc::encoder::encode(&packet)?;
                                    socket_out.send_to(&encoded, osc_out_addr).await?;
                                }
                                "ERROR" => {
                                    let args: Vec<OscType> =
                                        parts[1..].iter().map(|part| rcp_to_osc_arg(part)).collect();

                                    let msg = OscMessage {
                                        addr: "/error".to_string(),
                                        args,
                                    };

                                    println!("Sent OSC: {}", msg);

                                    // Convert to packet and send
                                    let packet = OscPacket::Message(msg);
                                    let encoded = rosc::encoder::encode(&packet)?;
                                    socket_out.send_to(&encoded, osc_out_addr).await?;
                                }
                                _ => {
                                    println!("Unsupported message type: {}", parts[0]);
                                }
                            }
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
    mut stream: tokio::net::tcp::OwnedWriteHalf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = [0u8; 1024];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, _addr)) => {
                if let Ok((_remaining, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                    match packet {
                        OscPacket::Message(msg) => {
                            println!("Received OSC: {}", msg);
                            let address = msg.addr;

                            // Split address and remove empty parts
                            let parts: Vec<&str> =
                                address.split('/').filter(|s| !s.is_empty()).collect();

                            if let Some(command) = parts.first() {
                                let rcp_command = format!("{} {}", command, parts[1..].join("/"));
                                let args: Vec<String> = msg
                                    .args
                                    .iter()
                                    .map(|arg| osc_to_rcp_arg(arg))
                                    .collect();
                                let rcp_command = format!("{} {}\n", rcp_command, args.join(" "));

                                println!("Sent RCP: {}", rcp_command);
                                if let Err(e) = stream.write_all(rcp_command.as_bytes()).await {
                                    eprintln!("Failed to write to RCP stream: {}", e);
                                }
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
