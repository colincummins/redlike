# redlike
Redlike is a concurrent, in-memory key–value store that communicates with clients over TCP using a simple, line-based text protocol.

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
* Each command ends with `\r\n`
* Responses are also line-based and terminated with `\r\n`
* Commands and keys are **case-sensitive**
* Values are treated as opaque strings

---

## Request Format

```
COMMAND [ARG1] [ARG2] ...\r\n
```

* Tokens are separated by **single spaces**
* Leading/trailing whitespace is ignored
* Empty lines are ignored

---

## Supported Commands

### `PING`

**Description:**
Health check command.

**Request:**

```
PING\r\n
```

**Response:**

```
PONG\r\n
```

---

### `GET key`

**Description:**
Retrieve the value associated with `key`.

**Request:**

```
GET mykey\r\n
```

**Responses:**

* If key exists:

  ```
  VALUE myvalue\r\n
  ```
* If key does not exist:

  ```
  NIL\r\n
  ```

---

### `SET key value`

**Description:**
Set `key` to `value`, overwriting any existing value.

**Request:**

```
SET mykey myvalue\r\n
```

**Response:**

```
OK\r\n
```

---

### `DEL key`

**Description:**
Delete a key if it exists.

**Request:**

```
DEL mykey\r\n
```

**Responses:**

* If key was deleted:

  ```
  OK\r\n
  ```
* If key did not exist:

  ```
  NIL\r\n
  ```

---

### `QUIT`

**Description:**
Close the client connection.

**Request:**

```
QUIT\r\n
```

**Response:**

```
BYE\r\n
```

The server closes the connection after sending the response.

---

## Error Handling

Errors are returned as explicit protocol responses.

### Error Response Format

```
ERROR <message>\r\n
```

### Examples

* Unknown command:

  ```
  ERROR unknown command\r\n
  ```

* Invalid argument count:

  ```
  ERROR invalid arguments\r\n
  ```

* Malformed request:

  ```
  ERROR protocol error\r\n
  ```

Errors do **not** close the connection unless otherwise specified.

---

## Concurrency Model

* Each client connection is handled asynchronously
* The underlying key–value store is shared safely across connections
* Commands are processed sequentially per connection

---

## Limits

* Maximum request line length: **8 KB**
* Keys and values must fit within a single request line
* Requests exceeding limits return:

  ```
  ERROR request too large\r\n
  ```

---

## Example Session

```
> PING
< PONG

> SET language rust
< OK

> GET language
< VALUE rust

> DEL language
< OK

> GET language
< NIL

> QUIT
< BYE
```
