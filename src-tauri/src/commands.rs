use crate::engines::DatabaseEngine;
use crate::{EngineMap, LogPayload, DbStatusPayload, LogCallback};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn create_database(
    server_id: String,
    db_type: String,
    host: String,
    port: u16,
    name: String,
    username: String,
    password: String,
    app: AppHandle,
    engines: State<'_, EngineMap>,
) -> Result<DbStatusPayload, String> {
    log::info!("Creating {} server '{}' on {}:{}", db_type, name, host, port);

    let pass = if password.is_empty() { None } else { Some(password.clone()) };

    let engine: Box<dyn DatabaseEngine> = match db_type.as_str() {
        "redis" => Box::new(crate::engines::redis::RedisEngine::new(pass)),
        "mongo" => Box::new(crate::engines::mongo::MongoEngine::new(pass)),
        "postgres" => Box::new(crate::engines::postgres::PostgresEngine::new(pass)),
        _ => return Err(format!("Unknown database type: {}", db_type)),
    };

    let mut map = engines.lock().await;
    if map.contains_key(&server_id) {
        return Err(format!("Server '{}' already exists", server_id));
    }

    let ae = app.clone();
    let sid = server_id.clone();
    let on_log: LogCallback = Arc::new(move |msg| {
        let _ = ae.emit("db:log", &LogPayload { db_type: sid.clone(), message: msg, level: "info".into() });
    });

    let ae2 = app.clone();
    let sid2 = server_id.clone();
    let on_debug: LogCallback = Arc::new(move |msg| {
        let _ = ae2.emit("db:debug", &LogPayload { db_type: sid2.clone(), message: msg, level: "debug".into() });
    });

    engine.start(host.clone(), port, on_log, on_debug).await?;

    map.insert(server_id.clone(), engine);

    let payload = DbStatusPayload { db_type: server_id.clone(), running: true, port, host, name, username };
    let _ = app.emit("db:status", &payload);
    Ok(payload)
}

#[tauri::command]
pub async fn stop_database(
    server_id: String,
    app: AppHandle,
    engines: State<'_, EngineMap>,
) -> Result<(), String> {
    log::info!("Stopping {}", server_id);
    let mut map = engines.lock().await;
    if let Some(engine) = map.remove(&server_id) {
        engine.stop().await?;
    }
    let _ = app.emit("db:status", &DbStatusPayload { db_type: server_id, running: false, port: 0, host: String::new(), name: String::new(), username: String::new() });
    Ok(())
}

#[tauri::command]
pub async fn wipe_database(
    server_id: String,
    engines: State<'_, EngineMap>,
) -> Result<(), String> {
    let map = engines.lock().await;
    if let Some(engine) = map.get(&server_id) { engine.wipe().await?; }
    Ok(())
}

#[tauri::command]
pub async fn get_db_status(
    engines: State<'_, EngineMap>,
) -> Result<Vec<DbStatusPayload>, String> {
    let map = engines.lock().await;
    Ok(map.iter().map(|(t, e)| DbStatusPayload {
        db_type: t.clone(),
        running: e.is_running(),
        port: 0,
        host: String::new(),
        name: String::new(),
        username: String::new(),
    }).collect())
}

#[tauri::command]
pub async fn execute_query(
    server_id: String,
    query: String,
    engines: State<'_, EngineMap>,
) -> Result<String, String> {
    let map = engines.lock().await;
    match map.get(&server_id) {
        Some(engine) => engine.execute_raw(query).await,
        None => Err(format!("Database server '{}' is not running", server_id)),
    }
}
