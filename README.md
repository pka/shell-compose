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
  - [ ] Config file for cron jobs (Justfile annotations?)
  - [x] Support task dependencies (via Justfile)
  - [x] Restarting failed jobs
  - [ ] Trigger execution by calling HTTP endpoint
* Observability
  - [x] Show process status
  - [x] Show process resource usage
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

## Similar projects

* [PM2](https://github.com/Unitech/pm2): Production process manager for Node.js/Bun applications with a built-in load balancer.

* [Pueue](https://github.com/Nukesor/pueue): Command-line task management tool for sequential and parallel execution of long-running tasks.

To start tasks in response to file modifications, consider using [watchexec](https://github.com/watchexec/watchexec).

For interactive background tasks, consider using [Pueue](https://github.com/Nukesor/pueue) or a terminal multiplexer like [Zellij](https://zellij.dev/), [tmux](http://tmux.github.io/) or [screen](https://www.gnu.org/software/screen/).

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

## Run with systemd and journal logging

Unit file example:
```
[Unit]
Description=Shell Compose
After=network.target
Requires=network.target

[Service]
Type=exec
User=user
Group=user
WorkingDirectory=/home/user/my-project-justfile-root
ExecStart=/usr/local/bin/shell-composed up autostart
Restart=always

[Install]
WantedBy=multi-user.target
```

The background process `shell-composed` supports a subset of the `shell-compose` command line arguments:
```
Usage: shell-composed [COMMAND]

Commands:
  run    Execute command
  runat  Execute command with cron schedule
  start  Start service
  up     Start service group
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```
