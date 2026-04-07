# Docker Guide

This repository includes a Docker image, a Compose setup, and a smoke test for
running Redlike as a containerized TCP service.

Redlike listens on port `6379` in the container. The provided
[`compose.yaml`](/home/colinc/redlike/compose.yaml) publishes that port to the
host, mounts a named Docker volume at `/data`, and sets
`ARCHIVE_PATH=/data/archive` so persisted state survives a clean container
restart.

## Quick Start

Build and start the service:

```bash
docker compose up --build
```

Redlike is then reachable on `127.0.0.1:6379`.

Because Redlike is a raw TCP service, use a Redis-compatible client, `nc`, or
another TCP tool rather than a browser.

Example `PING`:

```bash
printf '*1\r\n$4\r\nPING\r\n' | nc -N 127.0.0.1 6379
```

Expected response:

```text
+PONG
```

Stop the service:

```bash
docker compose down
```

Remove the service and its named volume:

```bash
docker compose down -v
```

## Persistence

The Compose setup uses a named volume:

```yaml
services:
  server:
    volumes:
      - data:/data

volumes:
  data:
```

That means archive data is stored in Docker-managed persistent storage, not in
a visible project directory. The archive is saved during graceful shutdown and
loaded again on startup.

The runtime image uses an entrypoint script to fix ownership on the mounted
`/data` volume before launching the server as the non-root `appuser`. That is
necessary because volume mounts replace the image-layer `/data` directory.

## Build the Image Directly

To build the image without Compose:

```bash
docker build -t redlike .
```

To run it directly:

```bash
docker run --rm -p 6379:6379 -e ADDRESS=0.0.0.0 -e PORT=6379 redlike
```

To enable archive persistence when running directly, mount writable storage and
set `ARCHIVE_PATH`, for example:

```bash
docker run --rm \
  -p 6379:6379 \
  -e ADDRESS=0.0.0.0 \
  -e PORT=6379 \
  -e ARCHIVE_PATH=/data/archive \
  -v redlike_data:/data \
  redlike
```

## Smoke Test

The repository includes a Docker smoke test script at
[`scripts/docker-smoke-test.sh`](/home/colinc/redlike/scripts/docker-smoke-test.sh).
It verifies that:

* the container starts
* `PING` returns `PONG`
* `SET` and `GET` work over RESP
* data persists across a clean `docker compose down` and restart

Run it locally with:

```bash
./scripts/docker-smoke-test.sh
```

On failure, the script prints `docker compose logs server` before cleaning up.

## CI

Docker smoke testing runs in GitHub Actions via
[`.github/workflows/docker.yml`](/home/colinc/redlike/.github/workflows/docker.yml).
That workflow is separate from the Rust workflow so container checks stay
independent from `cargo fmt`, `clippy`, and Rust test execution.
