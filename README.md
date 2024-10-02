# Shell Compose

Shell Compose is a lightweight background runner for long-running or scheduled tasks.

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

```
[group('autostart')]
processing:
  #!/usr/bin/env bash
  echo Start processing
  for i in {1..20}; do
    echo processing step $i
    sleep 1
  done
  echo Processing finished

[group('autostart')]
play:
  #!/usr/bin/env bash
  while read sound; do
    aplay sounds/$sound
  done < <(mosquitto_sub -t room/speaker)
```

Running a `just` recipe:
```
shell-compose start processing
```

Running all recipes in a group:
```
shell-compose up autostart
```
