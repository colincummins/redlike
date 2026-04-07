#!/usr/bin/env bash
set -e
trap 'docker compose down' EXIT
echo "Composing"
docker compose down -v
docker compose up --detach
echo "Sending PING"
response="$(printf '*1\r\n$4\r\nPING\r\n' | nc -N localhost 6379 | tr -d '\r')"
response="${response%$'\n'}"
expected="+PONG"
if [[ "$response" != "$expected" ]]; then
  printf 'Smoke test failed: expected %q, got %q\n' "$expected" "$response" >&2
  exit 1
fi
echo "SET mykey myvalue"
response="$(printf '*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n' | nc -N localhost 6379 | tr -d '\r')"
response="${response%$'\n'}"
expected="+OK"
if [[ "$response" != "$expected" ]]; then
  printf 'Smoke test failed: expected %q, got %q\n' "$expected" "$response" >&2
  exit 1
fi
echo "GET mykey"
response="$(printf '*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n' | nc -N localhost 6379 | tr -d '\r')"
response="${response%$'\n'}"
expected=$'$7\nmyvalue'
if [[ "$response" != "$expected" ]]; then
  printf 'Smoke test failed: expected %q, got %q\n' "$expected" "$response" >&2
  exit 1
fi
echo "Compose down..."
docker compose down
echo "Compose up again to check if archive is restored"
docker compose up --detach
echo "GET mykey"
response="$(printf '*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n' | nc -N localhost 6379 | tr -d '\r')"
response="${response%$'\n'}"
expected=$'$7\nmyvalue'
if [[ "$response" != "$expected" ]]; then
  printf 'Smoke test failed: expected %q, got %q\n' "$expected" "$response" >&2
  exit 1
fi
echo "Smoke test complete"

