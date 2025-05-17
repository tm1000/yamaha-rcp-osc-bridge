use rosc::{OscMessage, OscType};

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
