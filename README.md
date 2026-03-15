# redlike
Redlike is a concurrent, in-memory key-value store that communicates with clients over TCP using RESP, with optional inline terminal-style commands.

Implemented commands include `PING`, `GET`, `SET`, `DEL`, `EXPIRE`, `TTL`, and `QUIT`.
Expired keys are treated as missing on reads, and a background sweeper removes expired entries from the store.

The project is covered by unit tests, including deterministic Tokio paused-time tests for expiration and TTL behavior.

# API Specification

## Transport

* Protocol: TCP
* Default address: `127.0.0.1:6379`
* Connection model: persistent connections

---

## Protocol Overview

The server accepts two request formats:

* RESP arrays containing bulk strings, for example `*1\r\n$4\r\nPING\r\n`
* Inline commands terminated by `\n`, for example `PING\n`

Responses are always encoded as RESP frames.

Command names are case-insensitive. Keys and values are treated as raw bytes when sent as RESP bulk strings.

---

## Request Formats

### RESP

```
*<n>\r\n
$<len>\r\nCOMMAND\r\n
$<len>\r\nARG1\r\n
...
```

Examples:

```text
*1\r\n$4\r\nPING\r\n
*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n
*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n
*3\r\n$6\r\nEXPIRE\r\n$5\r\nmykey\r\n$2\r\n60\r\n
*2\r\n$3\r\nTTL\r\n$5\r\nmykey\r\n
```

### Inline

```text
COMMAND [ARG1] [ARG2]...\n
```

Examples:

```text
PING\n
GET mykey\n
SET mykey myvalue\n
EXPIRE mykey 60\n
TTL mykey\n
```

Blank inline lines are ignored.

---

## Supported Commands

### `PING`

Request:

```text
*1\r\n$4\r\nPING\r\n
```

or

```text
PING\n
```

Response:

```text
+PONG\r\n
```

---

### `GET key`

Request:

```text
*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n
```

Response when key exists:

```text
$7\r\nmyvalue\r\n
```

Response when key does not exist:

```text
$-1\r\n
```

Expired keys are treated as missing.

---

### `SET key value`

Request:

```text
*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n
```

Response:

```text
+OK\r\n
```

---

### `DEL key`

Request:

```text
*2\r\n$3\r\nDEL\r\n$5\r\nmykey\r\n
```

Response when key was deleted:

```text
:1\r\n
```

Response when key did not exist:

```text
:0\r\n
```

Expired keys are treated as missing.

---

### `EXPIRE key seconds`

Request:

```text
*3\r\n$6\r\nEXPIRE\r\n$5\r\nmykey\r\n$2\r\n60\r\n
```

Response when the timeout was set:

```text
:1\r\n
```

Response when the key does not exist or is already expired:

```text
:0\r\n
```

The timeout is specified in whole seconds. A timeout of `0` makes the key expire immediately.

---

### `TTL key`

Request:

```text
*2\r\n$3\r\nTTL\r\n$5\r\nmykey\r\n
```

Response when the key has an expiration:

```text
:<seconds>\r\n
```

Response when the key exists but has no expiration:

```text
:-1\r\n
```

Response when the key does not exist or is already expired:

```text
:-2\r\n
```

Returned TTL values are whole seconds.

---

### `QUIT`

Request:

```text
*1\r\n$4\r\nQUIT\r\n
```

The server closes the connection without sending a response frame.

---

## Error Handling

For valid request frames that contain an unknown command or the wrong number of arguments, the server replies with a RESP simple error:

```text
-Unknown Command\r\n
-Wrong number of arguments\r\n
```

If the input stream becomes malformed at the protocol level, the parser enters a terminal error state. Any frames completed before the error are still processed, then the connection is closed.

---

## Concurrency Model

* Each client connection is handled asynchronously.
* The underlying key-value store is shared across connections.
* Commands are processed sequentially per connection.

---

## Example Session

```text
> *1\r\n$4\r\nPING\r\n
< +PONG\r\n

> SET language rust\n
< +OK\r\n

> GET language\n
< $4\r\nrust\r\n

> EXPIRE language 60\n
< :1\r\n

> TTL language\n
< :60\r\n

> DEL language\n
< :1\r\n

> GET language\n
< $-1\r\n

> QUIT\n
< [connection closed]
```
