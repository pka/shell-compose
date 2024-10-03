# Shell Compose

Shell Compose is a lightweight background process runner for long-running or scheduled tasks.

![Shell Compose](https://raw.githubusercontent.com/pka/shell-compose/main/screencast.gif)

## Features

* Scheduling
  - [x] Run background tasks from the command line
  - [x] Run multiple tasks in parallel
  - [x] Schedule tasks to run like a cron job
  - [x] Start `just` recipes
  - [ ] Configure commands and cron jobs in a YAML file
  - [ ] Attach console to running task
  - [ ] Restarting failed tasks
* Logging
  - [x] Show logs of all running tasks
  - [ ] Show logs of selected tasks
* Cross Platform
  - [x] Linux
  - [x] MacOS
  - [x] Windows

## Integration with `just`

[just](https://just.systems/man/en/) is a command runner with syntax inspired by `make`.
It supports shell commands but also other languages like Python or NodeJS. 
Task can have dependencies and variables loaded from `.env` files.

Example:

```just
# Simulate data processing
[group('autostart')]
processing:
  #!/usr/bin/env bash
  echo Start processing
  for i in {1..20}; do
    echo processing step $i
    sleep 1
  done
  echo Processing finished

# Play sounds from MQTT queue
[group('autostart')]
play:
  #!/usr/bin/env bash
  set -euo pipefail
  mosquitto_sub -t room/speaker | while read sound; do
    aplay sounds/$sound
  done
```

Running a `just` recipe:
```
shell-compose start processing
```

Running all recipes in a group:
```
shell-compose up autostart
```

## Installation

### Pre-built binaries

We provide several options to access pre-built binaries for a variety of platforms. If you would like to manually download a pre-built binary, checkout [the latest release on GitHub](https://github.com/pka/shell-compose/releases/latest).

### Installer scripts

#### macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/pka/shell-compose/releases/latest/download/shell-compose-installer.sh | sh
```

#### Windows PowerShell:

```sh
powershell -c "irm https://github.com/pka/shell-compose/releases/latest/download/shell-compose-installer.ps1 | iex"
```

### Other Options

#### cargo-binstall

```sh
cargo binstall shell-compose
```

#### Build From Source

For users who need to install shell-compose on platforms that we do not yet provide pre-built binaries for, you will need to build from source.
`shell-compose` is written in [Rust](https://rust-lang.org) and uses [cargo](https://doc.rust-lang.org/cargo/index.html) to build. Once you've [installed the Rust toolchain (`rustup`)](https://rustup.rs/), run:

```sh
cargo install shell-compose --locked
```
