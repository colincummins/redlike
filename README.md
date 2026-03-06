# redlike
Redlike is a concurrent, in-memory key-value store that communicates with clients over TCP using a simple, line-based text protocol.

# API Specification

## Transport

* **Protocol:** TCP
* **Default Address:** `127.0.0.1:6379`
* **Connection Model:** Persistent connections
  A client may send multiple commands over a single connection.

---

## Protocol Overview

* Commands are **ASCII text**
* Commands are **line-based**
* Requests are terminated by a newline (`\n`)
* Responses are line-based and terminated with `\n`
* Commands are **case-insensitive**
* Keys are **case-sensitive**
* Values are parsed as a single token (no spaces)

---

## Request Format

```
COMMAND [ARG1] [ARG2] ...\n
```

* Tokens are separated by whitespace
* Leading/trailing whitespace is ignored
* Empty lines are ignored

---

## Supported Commands

### `PING`

**Description:**
Health check command.

**Request:**

```
PING\n
```

**Response:**

```
PONG\n
```

---

### `GET key`

**Description:**
Retrieve the value associated with `key`.

**Request:**

```
GET mykey\n
```

**Responses:**

* If key exists:

  ```
  myvalue\n
  ```
* If key does not exist:

  ```
  \n
  ```

---

### `SET key value`

**Description:**
Set `key` to `value`, overwriting any existing value.

**Request:**

```
SET mykey myvalue\n
```

**Response:**

```
OK\n
```

---

### `DEL key`

**Description:**
Delete a key if it exists.

**Request:**

```
DEL mykey\n
```

**Responses:**

* If key was deleted:

  ```
  1\n
  ```
* If key did not exist:

  ```
  0\n
  ```

---

### `QUIT`

**Description:**
Close the client connection.

**Request:**

```
QUIT\n
```

**Response:**
No response body is sent.

The server closes the connection immediately.

---

## Error Handling

Errors are returned as explicit protocol responses.

### Error Response Format

```
ERR <message>\n
```

### Examples

* Unknown command:

  ```
  ERR Unknown Command\n
  ```

* Invalid argument count:

  ```
  ERR Wrong number of arguments\n
  ```

Errors do **not** close the connection unless otherwise specified.

---

## Concurrency Model

* Each client connection is handled asynchronously
* The underlying key–value store is shared safely across connections
* Commands are processed sequentially per connection

---

## Limits

* Keys and values must fit within a single request line
* Value tokens cannot contain spaces

---

## Example Session

```
> PING
< PONG

> SET language rust
< OK

> GET language
< rust

> DEL language
< 1

> GET language
<

> QUIT
(connection closed)
```
