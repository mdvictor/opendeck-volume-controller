# OA Volume Controller Plugin

A per-application volume control plugin for Stream Deck using OpenDeck on Linux.

## Overview

Take full control of your sound experience with fine-tuned per-app volume management! This plugin integrates with PulseAudio to provide a visual mixer interface directly on your Stream Deck, allowing you to control individual application volumes with dedicated buttons.

## Features

- **Per-Application Volume Control**: Adjust volume levels for each running audio application independently
- **Visual Volume Bars**: Real-time graphical representation of volume levels on your Stream Deck
- **Mute Toggle**: Quickly mute/unmute applications with a single button press
- **System Mixer Support**: Optional system-wide mixer control
- **Auto-Detection**: Automatically discovers and tracks running audio applications
- **App Icons**: Displays application icons for easy identification
- **Real-time Updates**: Monitors PulseAudio events and updates the interface dynamically

## Usage

Drag the `Volume Control Auto Grid` action across the SD grid. This was tested and developed with the SD3x5 in mind, so at least one full column (3 actions per column) is needed to show one volume mixer, where the first action button is the mixer icon together with the mute/unmute button, the second is Vol+ and the remaining button is Vol-.

After setting your grid, switch profiles and return to your volume controller profile to kick things off.

## ToDo:

- Make this work on the SD+ so it responds to the dials and shows volume on the LCD?
- Figure out a layout for the 2-row SDs

### Grid Layout

The plugin uses a column-based layout on your Stream Deck:

- **Column 0**: Reserved for other actions (not used by volume control)
- **Columns 1+**: Each column represents one audio application

For each application column:

- **Row 0 (Top)**: Application icon/name - press to mute/unmute
- **Row 1 (Middle)**: Volume up button
- **Row 2 (Bottom)**: Volume down button

### Configuration

Access the Property Inspector for the action to configure:

- **Show system mixer**: Enable/disable system-wide audio device control

### Controls

- **Mute/Unmute**: Click the top button (row 0) to toggle mute
- **Increase Volume**: Click the middle button (row 1) to increase volume by 10%
- **Decrease Volume**: Click the bottom button (row 2) to decrease volume by 10%

## Building from Source

### Prerequisites

- Rust toolchain (edition 2024)
- PulseAudio development libraries
- pkg-config

### Build Steps

```bash
cargo build --release
```

The compiled binary will be available in `target/release/`

### Dependencies

- `pulsectl-rs` - PulseAudio control library
- `openaction` - Stream Deck plugin framework
- `tokio` - Async runtime
- `image` - Image processing for icons
- `tux-icons` - Linux application icon lookup

## Architecture

```
src/
├── main.rs       # Entry point
├── plugin.rs     # OpenAction plugin implementation
├── mixer.rs      # Mixer channel management
├── audio/        # PulseAudio integration
├── gfx.rs        # Graphics and image generation
└── utils.rs      # Utility functions
```

## Known Limitations

- Column 0 is currently reserved and won't display volume controls
- Maximum applications displayed is limited by Stream Deck columns (typically 8 columns = 7 apps)

## License

Author: Victor Marin

## Version

1.0.0
