use rosc::{OscMessage, OscPacket, OscType};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rcp_port = 49280;
    let rcp_host = "10.249.1.28";

    // OSC (UDP) settings
    let osc_out_addr = "127.0.0.1:8000";
    let osc_in_addr = "0.0.0.0:8001"; // Listen for incoming OSC on port 8001

    // Set up UDP sockets
    let socket_out = UdpSocket::bind("0.0.0.0:0").await?;
    let socket_in = Arc::new(UdpSocket::bind(osc_in_addr).await?);
    println!("Listening for OSC messages on: {}", osc_in_addr);
    println!("Sending OSC messages to: {}", osc_out_addr);

    // Connect TCP
    match TcpStream::connect((rcp_host, rcp_port)).await {
        Ok(stream) => {
            println!("Connected to Yamaha RCP: {}", rcp_host);
            let mut buffer = [0; 1024];
            let socket_in_clone = Arc::clone(&socket_in);
            let (tcp_read, tcp_write) = stream.into_split();

            // Spawn a task to handle incoming OSC messages
            tokio::spawn(async move { handle_incoming_osc(socket_in_clone, tcp_write).await });

            // Use tcp_read for the main loop
            let mut stream = tcp_read;

            loop {
                match stream.read(&mut buffer).await {
                    Ok(n) if n == 0 => {
                        println!("Connection closed by server");
                        break;
                    }
                    Ok(n) => {
                        let received = String::from_utf8_lossy(&buffer[..n]);
                        let parts: Vec<&str> = received.trim().split_whitespace().collect();
                        println!("Received RCP: {}", received.trim());

                        if parts[0] == "NOTIFY" || parts[0] == "OK" {
                            if parts.len() >= 5 {
                                println!("Parts: {:?}", parts);
                                let action = parts[1];
                                let osc_address = parts[2];
                                let x = parts[3].parse::<i32>().unwrap() + 1;
                                let y = parts[4].parse::<i32>().unwrap() + 1;
                                let type_tag = parts[5];
                                let value = if parts.len() > 6 { parts[6] } else { "" };

                                // /yosc:req/<Action>/<OSC address>/<X>/<Y> <type tag> <value>
                                // Construct OSC address pattern
                                let osc_addr_pattern = format!(
                                    "/yosc:req/{}/{}/{}/{} {} {}",
                                    action, osc_address, x, y, type_tag, value
                                );

                                // Create OSC message
                                let msg = OscMessage {
                                    addr: osc_addr_pattern.clone(),
                                    args: vec![OscType::String(value.to_string())],
                                };

                                // Convert to packet and send
                                let packet = OscPacket::Message(msg);
                                let encoded = rosc::encoder::encode(&packet)?;
                                socket_out.send_to(&encoded, osc_out_addr).await?;

                                println!("Sent OSC: {}", osc_addr_pattern);
                            } else if parts.len() == 4 {
                                let action = parts[1];
                                let osc_address = parts[2];
                                let x = parts[3];

                                //yosc:req/<Action> <OSC address> <value>
                                let osc_addr_pattern = format!(
                                    "/yosc:req/{} {} {}",
                                    action, osc_address, x
                                );

                                // Create OSC message
                                let msg = OscMessage {
                                    addr: osc_addr_pattern.clone(),
                                    args: vec![OscType::String(x.to_string())],
                                };

                                //yosc:req/ssrecallt_ex MIXER:Lib/Scene "5.00"
                                // Convert to packet and send
                                let packet = OscPacket::Message(msg);
                                let encoded = rosc::encoder::encode(&packet)?;
                                socket_out.send_to(&encoded, osc_out_addr).await?;

                                println!("Sent OSC: {}", osc_addr_pattern);
                            } else {
                                println!("Invalid message format: {}", received);
                            }
                        } else {
                            println!("Invalid message format: {}", received);
                        }

                        println!();
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
                            let address = msg.addr;
                            println!("Received OSCSSSS: {}", address);
                            let parts = address.split('/').collect::<Vec<&str>>();
                            let arg = msg.args.first();
                            if parts.len() >= 9 && parts[1] == "yosc:req" {
                                let action = parts[2];
                                let osc_address = parts[3..7].join("/");
                                let x = parts[7].parse::<i32>().unwrap() - 1;
                                let y = parts[8].parse::<i32>().unwrap() - 1;

                                // Convert OSC argument to string based on its type
                                let value_str = if let Some(value) = arg {
                                    match value {
                                        OscType::Int(i) => i.to_string(),
                                        OscType::Float(f) => f.to_string(),
                                        OscType::String(s) => s.clone(),
                                        OscType::Bool(b) => b.to_string(),
                                        _ => String::from("0"), // Default value for unsupported types
                                    }
                                } else {
                                    String::from("0")
                                };

                                let tcp_msg = format!(
                                    "{} {} {} {} {}\n",
                                    action, osc_address, x, y, value_str
                                );
                                println!("Sending RCP: {}", tcp_msg);
                                if let Err(e) = stream.write_all(tcp_msg.as_bytes()).await {
                                    eprintln!("Failed to write to RCP stream: {}", e);
                                }
                            } else {
                                println!("Invalid message format: {}", address);
                            }
                            println!();
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
