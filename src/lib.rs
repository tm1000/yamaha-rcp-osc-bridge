use rosc::{OscMessage, OscType};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Socket, Type};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

/// Configuration for running the Yamaha RCP <-> OSC bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// The remote console IP
    pub console_ip: String,
    /// The remote RCP port
    pub rcp_port: u16,
    /// The remote OSC address (IP or hostname)
    pub udp_osc_out_addr: String,
    /// The remote OSC port
    pub udp_osc_out_port: u16,
    /// The local OSC bind address
    pub udp_osc_in_addr: String,
    /// The local OSC bind port
    pub udp_osc_in_port: u16,
}

/// Severity of a log message, analogous to levels in other logging systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    /// Verbose, high-volume detail (e.g. raw RCP/OSC traffic) useful when
    /// diagnosing an issue but noisy in normal operation.
    Debug,
    /// Normal operational messages (connections, startup/shutdown).
    Info,
    /// Unexpected but non-fatal conditions (e.g. an unsupported message).
    Warn,
    /// A failure that prevented an operation from completing.
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        };
        write!(f, "{}", s)
    }
}

/// Type alias for a logging function that accepts a level and message
pub type LogFn = Box<dyn Fn(LogLevel, String) + Send + Sync>;

/// Run the Yamaha RCP <-> OSC bridge with the provided configuration.
///
/// This function connects to the Yamaha RCP TCP endpoint and bridges messages
/// to/from OSC over UDP. It runs until the TCP connection closes or an error occurs.
pub async fn run_bridge(
    config: BridgeConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_bridge_with_logger(
        config,
        Box::new(|level, msg| println!("[{}] {}", level, msg)),
    )
    .await
}

/// Run the bridge with a custom logging function.
///
/// The logger function will be called for all log messages instead of using println!
pub async fn run_bridge_with_logger(
    config: BridgeConfig,
    log: LogFn,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // RCP (TCP) settings
    let rcp_port = config.rcp_port;
    let rcp_host = config.console_ip.clone();

    // OSC (UDP) settings
    let osc_out_addr = format!("{}:{}", config.udp_osc_out_addr, config.udp_osc_out_port);
    let osc_in_addr = format!("{}:{}", config.udp_osc_in_addr, config.udp_osc_in_port);

    // Set up UDP sockets with SO_REUSEADDR to allow quick restart
    let socket_out = UdpSocket::bind("0.0.0.0:0").await?;

    // Create socket with reuse options for incoming OSC
    let addr: SocketAddr = osc_in_addr
        .parse()
        .map_err(|e| format!("Invalid OSC address: {}", e))?;
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;

    // On Unix systems, also set SO_REUSEPORT for immediate reuse
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    {
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        unsafe {
            let optval: libc::c_int = 1;
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of_val(&optval) as libc::socklen_t,
            );
        }
    }

    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    let std_socket: std::net::UdpSocket = socket.into();
    let socket_in = Arc::new(UdpSocket::from_std(std_socket)?);

    log(
        LogLevel::Info,
        format!("Listening for OSC messages on: {}", osc_in_addr),
    );
    log(
        LogLevel::Info,
        format!("Sending OSC messages to: {}", osc_out_addr),
    );
    log(
        LogLevel::Info,
        format!("Attempting to connect to Yamaha RCP: {}", rcp_host),
    );

    // Connect to TCP RCP
    match TcpStream::connect((rcp_host.clone(), rcp_port)).await {
        Ok(stream) => {
            log(
                LogLevel::Info,
                format!("Connected to Yamaha RCP: {}", rcp_host),
            );
            let mut buffer = [0; 1024];
            let socket_in_clone = Arc::clone(&socket_in);
            let (mut rcp_read, rcp_write) = stream.into_split();
            let rcp_write = Arc::new(Mutex::new(rcp_write));
            let rcp_write_clone = Arc::clone(&rcp_write);

            // Spawn a task to handle incoming OSC messages
            let log_clone = Arc::new(log);
            let log_for_osc = Arc::clone(&log_clone);
            tokio::spawn(async move {
                if let Err(_e) =
                    handle_incoming_osc(socket_in_clone, rcp_write_clone, log_for_osc).await
                {
                    // Error already logged in handle_incoming_osc
                }
            });

            //RCP commands can sometimes be sent in bundles and should be split by newline
            let mut incomplete_line = String::new();
            loop {
                match rcp_read.read(&mut buffer).await {
                    Ok(0) => {
                        log_clone(LogLevel::Warn, "Connection closed by server".to_string());
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buffer[..n]);
                        incomplete_line.push_str(&data);

                        // Process each complete line
                        while let Some(newline_pos) = incomplete_line.find('\n') {
                            let line = incomplete_line[..newline_pos].to_string();
                            incomplete_line = incomplete_line[newline_pos + 1..].to_string();
                            let parts = split_respecting_quotes(line.trim());

                            if parts.is_empty() {
                                continue;
                            }

                            log_clone(LogLevel::Debug, format!("Received RCP: {}", line.trim()));

                            let osc_message = match rcp_to_osc(line) {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    log_clone(
                                        LogLevel::Error,
                                        format!("Failed to convert RCP to OSC: {}", e),
                                    );
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
                                    log_clone(
                                        LogLevel::Error,
                                        format!("Failed to write to RCP stream: {}", e),
                                    );
                                }
                            }

                            log_clone(LogLevel::Debug, format!("Sending OSC: {}", osc_message));

                            // Convert to packet and send
                            let packet = rosc::OscPacket::Message(osc_message);
                            let encoded = rosc::encoder::encode(&packet)?;
                            socket_out.send_to(&encoded, osc_out_addr.clone()).await?;
                        }
                    }
                    Err(e) => {
                        log_clone(LogLevel::Error, format!("Failed to receive data: {}", e));
                        break;
                    }
                }
            }
        }
        Err(e) => {
            log(LogLevel::Error, format!("Failed to connect: {}", e));
            // Return error to stop the bridge gracefully
            return Err(format!("Connection failed: {}", e).into());
        }
    }

    Ok(())
}

async fn handle_incoming_osc(
    socket: Arc<UdpSocket>,
    stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    log: Arc<LogFn>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = [0u8; 1024];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, _addr)) => {
                if let Ok((_remaining, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                    match packet {
                        rosc::OscPacket::Message(msg) => {
                            log(LogLevel::Debug, format!("Received OSC: {}", msg));
                            let rcp_command = match osc_to_rcp(&msg) {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    log(
                                        LogLevel::Error,
                                        format!("Failed to convert OSC to RCP: {}", e),
                                    );
                                    continue;
                                }
                            };
                            log(LogLevel::Debug, format!("Sending RCP: {}", rcp_command));
                            if let Err(e) = stream
                                .lock()
                                .await
                                .write_all(format!("{}\n", rcp_command).as_bytes())
                                .await
                            {
                                log(
                                    LogLevel::Error,
                                    format!("Failed to write to RCP stream: {}", e),
                                );
                                continue;
                            }
                        }
                        rosc::OscPacket::Bundle(_) => {
                            log(
                                LogLevel::Warn,
                                "Received OSC bundle - not implemented".to_string(),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                log(
                    LogLevel::Error,
                    format!("Error receiving OSC message: {}", e),
                );
                break;
            }
        }
    }
    Ok(())
}

/// Converts a string argument from a Yamaha RCP command into an OSC type.
///
/// If the argument can be parsed as an i32, it is converted to an `OscType::Int`.
/// If the argument can be parsed as an f32, it is converted to an `OscType::Float`.
/// Otherwise, it is converted to an `OscType::String`.
pub fn rcp_to_osc_type(arg: &String) -> OscType {
    if let Ok(i) = arg.parse::<i32>() {
        OscType::Int(i)
    } else if let Ok(f) = arg.parse::<f32>() {
        OscType::Float(f)
    } else {
        OscType::String(arg.to_string())
    }
}

/// Splits a string into parts, respecting quotes.
///
/// This function splits the input string into parts, where each part is separated by a space.
/// However, if a part is enclosed in quotes, it is treated as a single part, even if it contains
/// spaces.
pub fn split_respecting_quotes(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes {
                    // Only add quotes when closing a quoted section
                    current.push(c);
                } else {
                    // Start a new quoted section
                    current.push(c);
                }
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    result.push(current);
                    current = String::new();
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Converts an OSC argument to a Yamaha RCP argument.
///
/// The RCP argument seems to be a string representation of the OSC argument.
///
/// # Errors
///
/// Returns an error if the OSC argument type is not supported.
pub fn osc_to_rcp_arg(arg: &OscType) -> Result<String, String> {
    match arg {
        OscType::Int(i) => Ok(i.to_string()),
        OscType::Float(f) => Ok(f.to_string()),
        OscType::String(s) => {
            // If the string is already quoted, return it as is
            if s.starts_with('"') && s.ends_with('"') {
                Ok(s.to_string())
            } else {
                Ok(format!("\"{}\"", s))
            }
        }
        _ => Err("Unsupported OSC type".to_string()),
    }
}

/// Converts an OSC message to a Yamaha RCP command.
///
/// The RCP command is in the format `<command> <argument1> <argument2> ...`
/// where `<command>` is the first part of the OSC address, and the
/// `<argumentN>` are the arguments of the OSC message.
///
/// # Errors
///
/// Returns an error if the OSC address is empty or invalid.
pub fn osc_to_rcp(msg: &OscMessage) -> Result<String, String> {
    let address = msg.addr.clone();

    // Split address and remove empty parts
    let parts: Vec<&str> = address.split('/').filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return Err("Invalid OSC address".to_string());
    }

    let rcp_command = format!("{} {}", parts[0], parts[1..].join("/"));
    let args: Result<Vec<String>, String> = msg.args.iter().map(osc_to_rcp_arg).collect();
    let args = args.map_err(|e| format!("Failed to convert OSC arg: {}", e))?;
    Ok(format!("{} {}", rcp_command, args.join(" ")))
}

/// Converts a Yamaha RCP message to an OSC message.
///
/// The RCP message is expected to be in one of the following formats:
/// * `NOTIFY <type> <name> <arg1> <arg2> ...`
/// * `OK <type> <name> <arg1> <arg2> ...`
/// * `ERROR <arg1> <arg2> ...`
///
/// The corresponding OSC messages are:
/// * `/type/name <arg1> <arg2> ...`
/// * `/type/name <arg1> <arg2> ...`
/// * `/error <arg1> <arg2> ...`
///
/// # Errors
///
/// Returns an error if the RCP message type is not supported.
pub fn rcp_to_osc(line: String) -> Result<OscMessage, String> {
    // Process the complete line
    let parts = split_respecting_quotes(line.trim());

    if parts.is_empty() {
        return Err("Invalid OSC address".to_string());
    }

    match parts[0].as_str() {
        "NOTIFY" | "OK" => {
            // Create OSC message
            let osc_addr_pattern = format!("/{}/{}", parts[1], parts[2]);

            let args: Vec<OscType> = parts[3..].iter().map(rcp_to_osc_type).collect();

            let msg: OscMessage = OscMessage {
                addr: osc_addr_pattern.clone(),
                args,
            };
            Ok(msg)
        }
        "ERROR" => {
            let args: Vec<OscType> = parts[1..].iter().map(rcp_to_osc_type).collect();

            let msg = OscMessage {
                addr: "/error".to_string(),
                args,
            };

            Ok(msg)
        }
        _ => Err("Unsupported message type".to_string()),
    }
}
