# Yamaha RCP to OSC Bridge

A Rust-based utility that bridges Yamaha RCP (Remote Control Protocol) commands to OSC (Open Sound Control) messages, enabling integration between Yamaha mixing consoles and OSC-compatible systems.

## Overview

This project translates Yamaha's proprietary RCP protocol into standardized OSC messages, making it possible to control Yamaha mixing consoles through OSC-compatible software and hardware. It's particularly useful for custom control solutions and integration with various audio control systems.

## References

This implementation is based on the following resources:

1. [Yamaha RCP Parameter Functions](https://github.com/bitfocus/companion-module-yamaha-rcp/blob/b0dfb601d142f3aa14120aad7561f8691641834e/paramFuncs.js#L150) - Reference implementation of RCP parameter handling
2. [Companion Module Implementation](https://github.com/search?q=repo%3Abitfocus%2Fcompanion-module-yamaha-rcp+sscurrent_ex&type=code) - Examples of RCP protocol usage
3. [QL Series SCP Commands Documentation](https://discourse.checkcheckonetwo.com/t/ql-series-scp-commands/2266/21) - Additional protocol documentation

## Installation

```bash
cargo install yamaha-rcp-to-osc
```

## Usage

1. Build the project:
   ```bash
   cargo build --release
   ```

2. Run the bridge:
   ```bash
   ./target/release/yamaha-rcp-to-osc [OPTIONS]
   ```

## Configuration

Detailed configuration options and examples coming soon.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.