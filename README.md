# Serbase

A cross-platform desktop app for running local database servers. Manage Redis, MongoDB, and PostgreSQL instances from a single UI — no Docker required.

Built with [Tauri v2](https://v2.tauri.app) (Rust backend + React frontend).

## Architecture

```
┌─────────────────────┐   ┌──────────────────────────────────┐
│  Frontend (React)   │   │  Backend (Rust / Tokio)          │
│                     │   │                                  │
│  MUI 6 + Zustand 5  │◄──►  Tauri Commands (invoke/events)  │
│  Vite               │   │                                  │
└─────────────────────┘   │  ┌────────────────────────────┐  │
                          │  │  DatabaseEngine trait       │  │
                          │  │  ├── RedisEngine (in-proc)  │  │
                          │  │  ├── MongoEngine (in-proc)  │  │
                          │  │  └── PostgresEngine (bin)   │  │
                          │  └────────────────────────────┘  │
                          └──────────────────────────────────┘
```

### Engine types

| Engine | Implementation | Details |
|---|---|---|
| **Redis** | In-process (custom RESP protocol) | No external binary needed |
| **MongoDB** | In-process (custom OP_MSG/OP_QUERY wire protocol) | No external binary needed |
| **PostgreSQL** | Spawns `postgres` + `initdb` binaries | Requires binaries on `$PATH` |

## Getting started

### Prerequisites

- **Rust** (stable) — [rustup.rs](https://rustup.rs)
- **Node.js** 20+
- **Tauri system deps** — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Install & run

```sh
npm ci
npx tauri dev
```

For frontend-only development:

```sh
npm run dev
# Opens Vite dev server on port 1420
```

### Commands

| Command | What it does |
|---|---|
| `npm run lint` | TypeScript type-check (`tsc --noEmit`) |
| `npm run build` | `tsc --noEmit && vite build` |
| `npx tauri dev` | Full Tauri dev mode (frontend + backend) |
| `cargo check` | Rust type-check (in `src-tauri/`) |

## Features

- **Create and manage** Redis, MongoDB, and PostgreSQL servers
- **Start / stop / wipe** instances from the sidebar
- **Real-time status** via Tauri events (`db:status`, `db:log`, `db:debug`)
- **Persistent** server definitions across restarts (`@tauri-apps/plugin-store`)
- **Tray icon** — app hides to menu bar on close (macOS)

## Project structure

```
src/                          # Frontend (React + TypeScript)
├── components/
│   ├── Common/               # Shared UI components
│   ├── Layout/               # App layout, sidebar, panels
│   └── ...
├── database/
│   └── types.ts              # DBType, server config types
├── store/
│   └── database-store.ts     # Zustand store + Tauri event listeners
├── App.tsx
└── main.tsx

src-tauri/                    # Backend (Rust)
├── src/
│   ├── commands.rs           # Tauri command handlers
│   ├── engines/
│   │   ├── mod.rs            # DatabaseEngine trait
│   │   ├── redis.rs          # In-process Redis implementation
│   │   ├── mongo.rs          # In-process MongoDB implementation
│   │   └── postgres.rs       # PostgreSQL binary management
│   ├── lib.rs                # App setup, tray, event handlers
│   └── main.rs               # Entry point
├── resources/                # Bundled assets (logos, etc.)
└── tauri.conf.json
```

## Testing connections with Python

After starting a server in Serbase, you can test the connection from Python using the provided test scripts. Install the required driver and run the appropriate script:

### Redis

```sh
pip install redis
```

```python
import redis

def test_redis_connection():
    try:
        # Connect to the local Redis server (default port is 6379)
        # decode_responses=True converts responses from bytes to strings
        r = redis.Redis(
            host='localhost', 
            port=6379, 
            db=0, 
            decode_responses=True,
            socket_connect_timeout=2  # Fails fast if the server is down
        )
        
        # Send a ping to the server
        if r.ping():
            print("Successfully connected to Redis!")
            
            # Optional: Perform a quick write/read test
            r.set("test_key", "Hello from Python!")
            print(f"Verified test data: {r.get('test_key')}")
            
    except redis.ConnectionError as e:
        print(f"Could not connect to Redis: {e}")

if __name__ == "__main__":
    test_redis_connection()
```

### MongoDB

```sh
pip install pymongo
```

```python
from pymongo import MongoClient
from pymongo.errors import ConnectionFailure, ServerSelectionTimeoutError

# Replace with your actual local or MongoDB Atlas connection string
MONGO_URI = "mongodb://localhost:27017/"

# Optional timeouts: Fail fast instead of waiting the default 30 seconds
client = MongoClient(MONGO_URI, serverSelectionTimeoutMS=3000)

try:
    # The ping command is cheap and does not require special auth privileges
    client.admin.command('ping')
    print("MongoDB connection successful!")
    
    # Optional: List available databases to verify read permissions
    print("Databases:", client.list_database_names())

except ServerSelectionTimeoutError:
    print("Connection failed: Server selection timed out.")
except ConnectionFailure:
    print("Connection failed: Server is unavailable or network error.")
except Exception as e:
    print(f"An unexpected error occurred: {e}")
finally:
    # Always close the connection pool when finished testing
    client.close()
```

### PostgreSQL

```sh
pip install psycopg2-binary
```

```python
import psycopg2


def test_connection():
    try:
        # Establish connection with database credentials
        connection = psycopg2.connect(
            dbname="your_db_name",
            user="your_username",
            password="your_password",
            host="localhost",  # Use IP or host string if remote
            port="5432",
        )

        # Create a cursor object to execute queries
        cursor = connection.cursor()

        # Run a simple test query
        cursor.execute("SELECT version();")

        # Fetch and print the server version
        db_version = cursor.fetchone()
        print("Success! Connected to PostgreSQL.")
        print(f"Database version: {db_version[0]}")

        # Clean up database resources
        cursor.close()
        connection.close()

    except Exception as error:
        print(f"Connection failed: {error}")


if __name__ == "__main__":
    test_connection()
```

> **Note:** Edit the host, port, and credentials in each script to match your server configuration in Serbase. The connection URL is shown in the toolbar when a server is running — use the copy button to grab it.

## Build & Release

Tag a commit with `v*` to trigger the CI pipeline (`.github/workflows/build.yml`):

- **macOS** — universal DMG (aarch64 + x86_64)
- **Linux** — `.deb` + `.AppImage`
- **Android** — `aarch64` APK
