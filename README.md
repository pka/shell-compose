<div class="oranda-hide">

# Shell Compose

</div>

Shell Compose is a lightweight background process runner for long-running or scheduled jobs.

![Shell Compose](https://raw.githubusercontent.com/pka/shell-compose/main/screencast.gif)

## Features

<pre>
* Scheduling
  - [x] Run background jobs from the command line
  - [x] Run multiple jobs in parallel
  - [x] Schedule commands to run like a cron job
  - [x] Start `just` recipes
  - [ ] Configure commands and cron jobs in a YAML file
  - [x] Support task dependencies (via Justfile)
  - [ ] Attach console to running job
  - [x] Restarting failed jobs
  - [ ] Trigger execution by file changes
  - [ ] Trigger execution by calling HTTP endpoint
* Observability
  - [x] Show process status
  - [ ] Show process resource usage
  - [x] Show logs of all running jobs
  - [x] Show logs of selected jobs
  - [ ] Metrics endpoint
* Cross Platform
  - [x] Linux
  - [x] MacOS
  - [x] Windows
</pre>

## Integration with `just`

[just](https://just.systems/man/en/) is a command runner with syntax inspired by `make`.
It supports shell commands but also other languages like Python or NodeJS. 
Tasks can have dependencies and variables loaded from `.env` files.

Example:

```make
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

# Serve current directory on port 8000
[group('autostart')]
serve:
  #!/usr/bin/env python3
  import http.server
  server_address = ('localhost', 8000)
  Handler = http.server.SimpleHTTPRequestHandler
  with http.server.HTTPServer(server_address, Handler) as httpd:
      print("Server started at http://%s:%s" % server_address, flush=True)
      httpd.serve_forever()
```

Running a `just` recipe:
```
shell-compose start processing
```

Running all recipes in a group:
```
shell-compose up autostart
```

<div class="oranda-hide">

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
powershell -ExecutionPolicy ByPass -c "irm https://github.com/pka/shell-compose/releases/latest/download/shell-compose-installer.ps1 | iex"
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

</div>
