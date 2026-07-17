use clap::Parser;
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

    let config = lib::BridgeConfig {
        console_ip: args.console_ip,
        rcp_port: args.rcp_port,
        udp_osc_out_addr: args.udp_osc_out_addr,
        udp_osc_out_port: args.udp_osc_out_port,
        udp_osc_in_addr: args.udp_osc_in_addr,
        udp_osc_in_port: args.udp_osc_in_port,
    };

    lib::run_bridge(config).await.map_err(|e| {
        let boxed: Box<dyn std::error::Error> = e;
        boxed
    })?;

    Ok(())
}
