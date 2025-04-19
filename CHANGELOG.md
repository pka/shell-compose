# Changelog

## 0.3.2 - 2025-04-19

- Exponential backoff for restarting jobs
- LOC: 1417

## 0.3.1 - 2025-04-18

- Create process groups
- Emit cli error as log message
- LOC: 1409

## 0.3.0 - 2024-10-13

- Collect system stats for running processes
- Create a background service instance per user
- Restrict access permissions for communication socket
- Initial work for Windows Security Context
- Differentate restart policies for job types
- Fix restart of failed services
- LOC: 1404

## 0.2.2 - 2024-10-12

- Fix background process termination with Ctrl-C on Windows

## 0.2.1 - 2024-10-10

- Fix log output without filter
- Sort buffered log entries by timestamp

## 0.2.0 - 2024-10-09

- `jobs` command listing active jobs
- `stop` command for commands, services and cron jobs
- `down` command for service groups
- Handle services as single-instance jobs
- Respawn processes with exit status > 0
- LOC: 1145

## 0.1.3 - 2024-10-04

- Stop waiting for new logs when client disconnects

## 0.1.2 - 2024-10-03

### New features

- aarch64-unknown-linux-gnu binary package

### Fixes

- Check for terminal color support

## 0.1.1 - 2024-10-03

- Fix background process filename on Windows

## 0.1.0 - 2024-10-03

- First public release
- LOC: 808
