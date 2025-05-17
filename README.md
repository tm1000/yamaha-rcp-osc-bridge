# Yamaha RCP to OSC Bridge

A Rust-based utility that bridges Yamaha RCP (Remote Control Protocol) commands to OSC (Open Sound Control) messages, enabling integration between Yamaha mixing consoles and OSC-compatible systems.

## Overview

This project translates Yamaha's proprietary RCP protocol into standardized OSC messages, making it possible to control Yamaha mixing consoles through OSC-compatible software and hardware. It's particularly useful for custom control solutions and integration with various audio control systems.

## Why

You might be surprised to know that Yamaha does not provide a real-time interface to their consoles. The only way to get real-time data is to use the RCP protocol, which is a proprietary protocol that is not documented by Yamaha. This project provides a bridge between the RCP protocol and OSC, making it possible to control Yamaha mixing consoles through OSC-compatible software and hardware.

Additionally this provides a small work around for Yamaha's RCP protocol which does not give detailed information when a notification of `sscurrent_ex` is received. In this case this utility will send a `ssinfo_ex` command to get the current scene information.

Any commands send over OSC are passed back to this library as is.

## Usage

### Generic

```bash
yamaha-rcp-to-osc --console-ip 192.168.69.165
```

### Vor

Example for DM3

Create a Custom OSC connection.

Change `Address 1` to `/ssinfo_ex/scene_a`

Create a layout from Custom OSC with the label set to `Console: %1:2 %1:3`

```bash
yamaha-rcp-to-osc --console-ip 192.168.69.165 --udp-osc-out-port 5003
```



## TODO

- [ ] Add support TCP OSC
- [ ] Add 1.1 OSC Support [https://github.com/klingtnet/rosc/pull/62](https://github.com/klingtnet/rosc/pull/62)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## References

This implementation is based on the following resources:

1. [Companion Module Implementation](https://github.com/bitfocus/companion-module-yamaha-rcp) - Examples of RCP protocol usage
2. [QL Series SCP Commands Documentation](https://discourse.checkcheckonetwo.com/t/ql-series-scp-commands/2266/21) - Additional protocol discussion
3. [Yamaha RCP Protocol Documentation](https://my.yamaha.com/files/download/other_assets/8/1623778/DME7_remote_control_protocol_spec_v100_en.pdf) - Yamaha RCP Protocol Documentation
4. [yamaha-rcp-docs](https://github.com/BrenekH/yamaha-rcp-docs) - Additional protocol discussion