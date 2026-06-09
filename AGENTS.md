# Serbase

Tauri v2 desktop app that runs local database servers (Redis, MongoDB, PostgreSQL).

## Commands

| Command | What it does |
|---|---|
| `npm run lint` | TypeScript type-check (`tsc --noEmit`) — actual CI lint gate |
| `npm run build` | `tsc --noEmit && vite build` |
| `npm run dev` | Vite dev server only (port 1420). Use `npx tauri dev` for full app |
| `npx tauri dev` | Full Tauri dev mode (frontend + Rust backend) |
| `cargo check` | Rust type-check (in `src-tauri/`) |

CI runs `tsc --noEmit` + `cargo check`. No test framework exists.

## Architecture

- **Frontend** (`src/`): React 18 + TypeScript + MUI 6 + Zustand 5
- **Backend** (`src-tauri/src/`): Rust with tokio
- **3 engine types**, all managed via the `DatabaseEngine` trait (`engines/mod.rs`):
  - **Redis** (`engines/redis.rs`): custom in-process RESP protocol implementation — no external binary needed
  - **MongoDB** (`engines/mongo.rs`): custom in-process OP_MSG/OP_QUERY wire protocol implementation — no external binary needed
  - **PostgreSQL** (`engines/postgres.rs`): spawns real `postgres` + `initdb` binaries via `tokio::process::Command` — requires them on `$PATH`
- **Tauri commands** (`commands.rs`): `create_database`, `stop_database`, `wipe_database`, `execute_query`, `get_db_status`
- **State** (`store/database-store.ts`): Zustand store + Tauri event listeners (`db:log`, `db:debug`, `db:status`) for real-time UI updates
- **Persistence**: server definitions saved via `@tauri-apps/plugin-store` to `serbase-servers.json`
- **Tray icon**: app hides on close on macOS (tray pattern with "Open App" / "Quit" menu)
- **No tests exist** — no test framework, no test files

## Key details

- Default server host in create dialog is `127.0.0.1`. Set to `0.0.0.0` to accept connections from other devices.
- Vite watcher ignores `src-tauri/` (mounted from `vite.config.ts`).
- lib crate (`src-tauri/src/lib.rs`) exports `run()`, called from `main.rs`.
- Server state (`EngineMap`) is `Arc<Mutex<HashMap<String, Box<dyn DatabaseEngine>>>>` managed as Tauri state.
- CSP in `tauri.conf.json` is permissive (allows `unsafe-eval`, `unsafe-inline`, `blob:`).
