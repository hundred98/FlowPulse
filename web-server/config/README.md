# Web Server Configuration

This directory contains configuration files for the FlowPulse Web Server.

## Files

- `web-server.json` - Actual configuration file (used by the server)
- `web-server.example.json` - Example configuration with comments (template)

## Usage

1. Copy the example file:
   ```bash
   cp web-server.example.json web-server.json
   ```

2. Edit `web-server.json` with your settings:
   ```bash
   nano web-server.json
   ```

3. Start the server (config will be loaded automatically):
   ```bash
   ./host
   ```

## Configuration Options

See `web-server.example.json` for detailed comments on each option.

For complete documentation, see:
- [Web服务器配置说明](../../docs/Web服务器配置说明.md)
