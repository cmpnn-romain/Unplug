<a name="readme-top"></a>

<br />
<div align="center">
  <img src="assets/unplug-icon.png" alt="Unplug" width="140">
  <h3 align="center">Unplug</h3>
  <p align="center">Block USB devices so no application can use them.</p>
</div>

## Table of Contents

- [About The Project](#about-the-project)
  - [Built With](#built-with)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Installation](#installation)
- [Usage](#usage)
- [Credits](#credits)

## About The Project

**Unplug** is for the joystick that keeps moving the game when nobody touches it. Some games read every controller they find, and the only
way out is to pull the cable. Unplug lists your USB devices and turns the ones you pick off at the driver level (what Device Manager calls
*Disable device*), so no application can read them. Unblock a device and it is back, without touching the cable.

Unplug is an **unofficial**, independent tool. It shares its look, build and release system with the [Cairn launcher](https://github.com/Cairn-client/Launcher).

### Built With

* [![Rust](https://img.shields.io/badge/Rust-tool-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/) with [egui](https://github.com/emilk/egui) for the window

## Getting Started

### Prerequisites

* Windows 10 or 11

### Installation

Download the latest `Unplug-X.Y.Z.exe` from the [Releases](https://github.com/cmpnn-romain/Unplug/releases) page and run it. There is nothing to install.

The program is not code-signed yet, so Windows may warn: click **More info**, then **Run anyway**. It asks for administrator rights, because
turning a device off needs them.

## Usage

Press **Block** on the joystick: it stops being available to every application. Press **Unblock** when you want it back, or **Unblock all**.

- Controllers (joysticks, pads, wheels, throttles, pedals) are listed first. A device that does not look like one asks before it is blocked.
- A keyboard or a mouse cannot be blocked from here, so the tool cannot lock you out. Hubs are never listed.
- A blocked device stays blocked after you close the window and after a restart, until you unblock it. Without this tool, open Device Manager,
  find the device and choose *Enable device*.
- `unplug --demo` runs the window on a made-up list that changes in memory only.

## Credits

See [AUTHORS.md](AUTHORS.md) for the complete list of contributors and the lineage.

## License

Unplug is free software under the [GNU General Public License v3.0](LICENSE).

<p align="right">(<a href="#readme-top">back to top</a>)</p>
