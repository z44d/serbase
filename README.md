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

## Build & Release

Tag a commit with `v*` to trigger the CI pipeline (`.github/workflows/build.yml`):

- **macOS** — universal DMG (aarch64 + x86_64)
- **Linux** — `.deb` + `.AppImage`
- **Android** — `aarch64` APK
