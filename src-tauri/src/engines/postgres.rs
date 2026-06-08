use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use super::{DatabaseEngine, LogCallback};

pub struct PostgresEngine {
    db: Arc<Mutex<rusqlite::Connection>>,
    running: Arc<AtomicBool>,
    stop_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    password: Arc<Option<String>>,
}

impl PostgresEngine {
    pub fn new(password: Option<String>) -> Self {
        let conn = rusqlite::Connection::open_in_memory().expect("Failed to create in-memory SQLite");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS pg_class (oid INTEGER PRIMARY KEY, relname TEXT, relkind TEXT)").ok();
        Self {
            db: Arc::new(Mutex::new(conn)),
            running: Arc::new(AtomicBool::new(false)),
            stop_tx: Arc::new(Mutex::new(None)),
            password: Arc::new(password),
        }
    }

    async fn handle_connection(stream: &mut TcpStream, db: Arc<Mutex<rusqlite::Connection>>, password: Arc<Option<String>>, on_debug: LogCallback) {
        let mut buf = vec![0u8; 65536];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = &buf[..n];
                    if data.len() < 5 { break; }

                    // Check for SSLRequest: length=8, request_code=80877103 (0x000004D2)
                    if data.len() >= 8 {
                        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                        if len == 8 && code == 80877103 {
                            on_debug("SSLRequest received, refusing SSL".into());
                            let _ = stream.write_all(b"N").await;
                            continue;
                        }
                    }

                    let is_startup = data[0] == 0;
                    let (msg_type, msg_len, payload) = if is_startup {
                        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                        ('\0', len, &[][..])
                    } else {
                        let mtype = data[0] as char;
                        let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
                        let pl = if data.len() > 5 { &data[5..(len + 1).min(data.len())] } else { &[] };
                        (mtype, len, pl)
                    };
                    on_debug(format!(">> PG msg_type={}, len={}", msg_type, msg_len));

                    match msg_type {
                        // Startup message
                        '\0' | 'p' => {
                            // Check password if configured
                            if let Some(expected) = password.as_ref() {
                                if msg_type == '\0' {
                                    // Send AuthenticationCleartextPassword (type 3)
                                    let auth_req = Self::build_pg_msg(b'R', &[0,0,0,3]);
                                    let _ = stream.write_all(&auth_req).await;
                                    continue;
                                } else {
                                    // Password message - verify
                                    let pw = if payload.last() == Some(&0) {
                                        String::from_utf8_lossy(&payload[..payload.len()-1]).to_string()
                                    } else {
                                        String::from_utf8_lossy(payload).to_string()
                                    };
                                    if pw != *expected {
                                        let mut f = vec![b'S']; f.extend_from_slice(b"FATAL"); f.push(0);
                                        f.push(b'M'); f.extend_from_slice(b"password authentication failed"); f.push(0);
                                        f.push(0);
                                        let _ = stream.write_all(&Self::build_pg_msg(b'E', &f)).await;
                                        return;
                                    }
                                }
                            }
                            // Auth OK
                            let auth = Self::build_pg_msg(b'R', &[0,0,0,0]);
                            let _ = stream.write_all(&auth).await;
                            for (k, v) in &[("server_version","16.0"),("server_encoding","UTF8"),("client_encoding","UTF8"),("DateStyle","ISO, MDY")] {
                                let p = format!("{}\0{}\0", k, v).into_bytes();
                                let _ = stream.write_all(&Self::build_pg_msg(b'S', &p)).await;
                            }
                            let _ = stream.write_all(&Self::build_pg_msg(b'K', &[0,0,0,1,0,0,0,1])).await;
                            let _ = stream.write_all(&Self::build_pg_msg(b'Z', b"I")).await;
                        }
                        'Q' => {
                            let sql = if payload.last() == Some(&0) { String::from_utf8_lossy(&payload[..payload.len()-1]).to_string() } else { String::from_utf8_lossy(payload).to_string() };
                            on_debug(format!("Query: {}", sql));
                            let trimmed = sql.trim().to_uppercase();
                            let is_select = trimmed.starts_with("SELECT") || trimmed.starts_with("WITH") || trimmed.starts_with("SHOW") || trimmed.starts_with("PRAGMA");
                            let is_insert = trimmed.starts_with("INSERT");
                            let is_create = trimmed.starts_with("CREATE") || trimmed.starts_with("ALTER") || trimmed.starts_with("DROP");

                            // Eagerly collect all results before any await to avoid Send issues with rusqlite
                            let (row_data, error, _has_data) = {
                                let conn = db.lock().await;
                                if is_select {
                                    match conn.prepare(&sql) {
                                        Ok(mut stmt) => {
                                            let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
                                            match stmt.query_map([], |row| {
                                                let mut vals = Vec::new();
                                                for i in 0..cols.len() {
                                                    let val: String = row.get::<_, String>(i).unwrap_or_default();
                                                    vals.push(val);
                                                }
                                                Ok(vals)
                                            }) {
                                                Ok(rows) => {
                                                    let mut all_rows: Vec<Vec<String>> = Vec::new();
                                                    for row in rows {
                                                        if let Ok(vals) = row {
                                                            all_rows.push(vals);
                                                        }
                                                    }
                                                    (Some((cols, all_rows)), None, true)
                                                }
                                                Err(e) => (None, Some(e.to_string()), false),
                                            }
                                        }
                                        Err(e) => (None, Some(e.to_string()), false),
                                    }
                                } else {
                                    match conn.execute_batch(&sql) {
                                        Ok(_) => (None, None, false),
                                        Err(e) => (None, Some(e.to_string()), false),
                                    }
                                }
                            };

                            // Now do all async writes
                            if let Some(err) = error {
                                let mut f = vec![b'S']; f.extend_from_slice(b"ERROR"); f.push(0);
                                f.push(b'M'); f.extend_from_slice(err.as_bytes()); f.push(0);
                                f.push(0);
                                let _ = stream.write_all(&Self::build_pg_msg(b'E', &f)).await;
                            } else if let Some((cols, all_rows)) = row_data {
                                // Send row description
                                let mut rd = Vec::new();
                                rd.extend_from_slice(&(cols.len() as u16).to_be_bytes());
                                for col in &cols {
                                    rd.extend_from_slice(col.as_bytes());
                                    rd.push(0);
                                    rd.extend_from_slice(&[0,0,0,0,0,0]);
                                    rd.extend_from_slice(&[0,0]);
                                    rd.extend_from_slice(&[0,0,0,25]);
                                    rd.extend_from_slice(&[0,0,0,4]);
                                    rd.extend_from_slice(&[0,0,0,0]);
                                    rd.extend_from_slice(&[0,0]);
                                }
                                let _ = stream.write_all(&Self::build_pg_msg(b'T', &rd)).await;

                                let row_count = all_rows.len() as i64;
                                for vals in &all_rows {
                                    let mut dr = Vec::new();
                                    dr.extend_from_slice(&(vals.len() as u16).to_be_bytes());
                                    for val in vals {
                                        let bytes = val.as_bytes();
                                        dr.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                                        dr.extend_from_slice(bytes);
                                    }
                                    let _ = stream.write_all(&Self::build_pg_msg(b'D', &dr)).await;
                                }
                                let tag = if is_insert { format!("INSERT 0 {}", row_count) } else { format!("SELECT {}", row_count) };
                                let _ = stream.write_all(&Self::build_pg_msg(b'C', format!("{}\0", tag).as_bytes())).await;
                            } else {
                                let tag = if is_create { "CREATE TABLE" } else { "OK" };
                                let _ = stream.write_all(&Self::build_pg_msg(b'C', format!("{}\0", tag).as_bytes())).await;
                            }
                            let _ = stream.write_all(&Self::build_pg_msg(b'Z', b"I")).await;
                        }
                        'X' => break,
                        _ => on_debug(format!("Unhandled PG msg: {}", msg_type)),
                    }
                }
                Err(e) => { log::error!("PG read error: {}", e); break; }
            }
        }
    }

    fn build_pg_msg(msg_type: u8, body: &[u8]) -> Vec<u8> {
        let len = 4 + body.len();
        let mut msg = Vec::with_capacity(1 + len);
        msg.push(msg_type);
        msg.extend_from_slice(&(len as u32).to_be_bytes());
        msg.extend_from_slice(body);
        msg
    }
}

impl DatabaseEngine for PostgresEngine {
    fn start(&self, host: String, port: u16, on_log: LogCallback, on_debug: LogCallback) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let db = self.db.clone(); let running = self.running.clone(); let stop_tx = self.stop_tx.clone();
        let password = self.password.clone();
        let addr = format!("{}:{}", host, port);
        Box::pin(async move {
            if running.load(Ordering::SeqCst) { return Err("PostgreSQL is already running".into()); }
            let listener = TcpListener::bind(&addr).await.map_err(|e| format!("Failed to bind PostgreSQL on {}: {}", addr, e))?;
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            *stop_tx.lock().await = Some(tx); running.store(true, Ordering::SeqCst);
            on_log(format!("PostgreSQL listening on {}", addr));
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        accept = listener.accept() => { match accept { Ok((mut stream, addr)) => { on_debug(format!("New PG connection from {}", addr)); let d = db.clone(); let dbg = on_debug.clone(); let p = password.clone(); tokio::spawn(async move { PostgresEngine::handle_connection(&mut stream, d, p, dbg).await; }); } Err(e) => { log::error!("PG accept error: {}", e); break; } } }
                        _ = &mut rx => break,
                    }
                }
                running.store(false, Ordering::SeqCst); on_log("PostgreSQL stopped".into());
            });
            Ok(())
        })
    }
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let stop_tx = self.stop_tx.clone();
        Box::pin(async move { if let Some(tx) = stop_tx.lock().await.take() { let _ = tx.send(()); } Ok(()) })
    }
    fn wipe(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let db = self.db.clone();
        Box::pin(async move { let conn = db.lock().await; conn.execute_batch("DROP TABLE IF EXISTS pg_class").ok(); Ok(()) })
    }
    fn execute_raw(&self, query: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let db = self.db.clone();
        Box::pin(async move {
            let conn = db.lock().await;
            let trimmed = query.trim().to_uppercase();
            let is_select = trimmed.starts_with("SELECT") || trimmed.starts_with("WITH") || trimmed.starts_with("SHOW") || trimmed.starts_with("PRAGMA");
            if is_select {
                let mut stmt = conn.prepare(&query).map_err(|e| format!("Query error: {}", e))?;
                let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
                let rows: Vec<serde_json::Value> = stmt.query_map([], |row| {
                    let mut map = serde_json::Map::new();
                    for (i, col) in cols.iter().enumerate() {
                        let val: String = row.get::<_, String>(i).unwrap_or_default();
                        map.insert(col.clone(), serde_json::Value::String(val));
                    }
                    Ok(serde_json::Value::Object(map))
                }).map_err(|e| format!("Query error: {}", e))?.filter_map(|r| r.ok()).collect();
                let row_count = rows.len();
                Ok(serde_json::json!({"command": "SELECT", "rowCount": row_count, "columns": cols, "rows": rows}).to_string())
            } else {
                conn.execute_batch(&query).map_err(|e| format!("Query error: {}", e))?;
                let tag = if trimmed.starts_with("INSERT") { "INSERT" } else if trimmed.starts_with("CREATE") { "CREATE TABLE" } else { "OK" };
                Ok(serde_json::json!({"command": tag, "rowCount": 0, "columns": [], "rows": []}).to_string())
            }
        })
    }
    fn is_running(&self) -> bool { self.running.load(Ordering::SeqCst) }
}
