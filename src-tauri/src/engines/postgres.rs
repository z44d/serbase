use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use super::{DatabaseEngine, LogCallback};

fn find_binary(name: &str) -> Option<String> {
    let candidates = vec![
        format!("/usr/local/bin/{}", name),
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/bin/{}", name),
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.clone());
        }
    }
    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            let bin = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !bin.is_empty() && std::path::Path::new(&bin).exists() {
                return Some(bin);
            }
        }
    }
    None
}

fn default_data_dir(host: &str, port: u16) -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    Ok(PathBuf::from(&home)
        .join(".serbase")
        .join("data")
        .join(format!("pg_{}_{}", host, port)))
}

pub struct PostgresEngine {
    running: Arc<AtomicBool>,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    conn_info: Arc<Mutex<Option<(String, u16)>>>,
    data_dir: Arc<Mutex<Option<PathBuf>>>,
    pg_bin: Arc<Option<String>>,
    initdb_bin: Arc<Option<String>>,
    username: Arc<String>,
    dbname: Arc<String>,
    password: Arc<Option<String>>,
}

impl PostgresEngine {
    pub fn new(password: Option<String>, username: String, dbname: String) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
            conn_info: Arc::new(Mutex::new(None)),
            data_dir: Arc::new(Mutex::new(None)),
            pg_bin: Arc::new(find_binary("postgres")),
            initdb_bin: Arc::new(find_binary("initdb")),
            username: Arc::new(username),
            dbname: Arc::new(dbname),
            password: Arc::new(password),
        }
    }
}

impl DatabaseEngine for PostgresEngine {
    fn start(
        &self,
        host: String,
        port: u16,
        on_log: LogCallback,
        on_debug: LogCallback,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let pg = self.pg_bin.clone();
        let initdb = self.initdb_bin.clone();
        let running = self.running.clone();
        let child = self.child.clone();
        let conn_info = self.conn_info.clone();
        let dd = self.data_dir.clone();
        let su = self.username.clone();
        let sdb = self.dbname.clone();
        let spw = self.password.clone();
        Box::pin(async move {
            if running.load(Ordering::SeqCst) {
                return Err("PostgreSQL is already running".into());
            }

            let pg_bin = pg.as_ref().as_deref()
                .ok_or_else(|| "postgres binary not found. Install PostgreSQL and try again.".to_string())?;
            let initdb_bin = initdb.as_ref().as_deref()
                .ok_or_else(|| "initdb binary not found. Install PostgreSQL and try again.".to_string())?;

            let data_dir = default_data_dir(&host, port)?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("Failed to create data directory: {}", e))?;

            if !data_dir.join("PG_VERSION").exists() {
                on_log("Initializing PostgreSQL database cluster...".into());
                let status = Command::new(initdb_bin)
                    .arg("-D").arg(&data_dir)
                    .arg("--auth=trust")
                    .arg("--username=postgres")
                    .arg("--encoding=UTF8")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .status().await
                    .map_err(|e| format!("initdb failed: {}", e))?;
                if !status.success() {
                    return Err("initdb failed to initialize the database cluster".into());
                }
            }

            on_log(format!("Starting PostgreSQL on {}:{}", host, port));
            let mut proc = Command::new(pg_bin)
                .arg("-D").arg(&data_dir)
                .arg("-p").arg(port.to_string())
                .arg("-h").arg(&host)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("Failed to start PostgreSQL: {}", e))?;

            let stderr = proc.stderr.take().ok_or("No stderr from postgres")?;
            let ol = on_log.clone();
            let od = on_debug.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if line.contains("FATAL") || line.contains("ERROR") {
                        ol(format!("[PG] {}", line));
                    } else {
                        od(format!("[PG] {}", line));
                    }
                }
            });

            // Wait for PostgreSQL to be ready (up to 10 seconds)
            let conn_str = format!(
                "host={} port={} user=postgres dbname=postgres connect_timeout=2",
                host, port
            );
            let mut ready = false;
            for i in 0..20 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if let Ok((_client, connection)) =
                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await
                {
                    tokio::spawn(async move { connection.await });
                    ready = true;
                    break;
                }
                on_debug(format!("Waiting for PostgreSQL (attempt {})...", i + 1));
            }
            if !ready {
                let _ = proc.kill().await;
                let _ = proc.wait().await;
                return Err("PostgreSQL failed to start within timeout".into());
            }

            *child.lock().await = Some(proc);
            *conn_info.lock().await = Some((host.clone(), port));
            *dd.lock().await = Some(data_dir);
            running.store(true, Ordering::SeqCst);
            on_log("PostgreSQL started and ready".into());

            // Create the configured user role and database
            let conn_str = format!(
                "host={} port={} user=postgres dbname=postgres connect_timeout=5",
                host, port
            );
            if let Ok((client, connection)) =
                tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await
            {
                tokio::spawn(async move { connection.await });
                let username = &*su;
                let dbname = &*sdb;
                let password = &*spw;
                let db = if dbname.is_empty() { username } else { dbname.as_str() };

                if username != "postgres" {
                    let create_role = if let Some(pwd) = password {
                        format!("CREATE ROLE \"{}\" WITH LOGIN SUPERUSER PASSWORD '{}'", username, pwd)
                    } else {
                        format!("CREATE ROLE \"{}\" WITH LOGIN SUPERUSER", username)
                    };
                    match client.simple_query(&create_role).await {
                        Ok(_) => on_log(format!("Created role \"{}\"", username)),
                        Err(e) => on_debug(format!("Note (role): {}", e)),
                    }
                }

                if db != "postgres" {
                    let create_db = format!("CREATE DATABASE \"{}\" OWNER \"{}\"", db, username);
                    match client.simple_query(&create_db).await {
                        Ok(_) => on_log(format!("Created database \"{}\"", db)),
                        Err(e) => on_debug(format!("Note (database): {}", e)),
                    }
                }
            }

            Ok(())
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let child = self.child.clone();
        let running = self.running.clone();
        Box::pin(async move {
            if let Some(mut proc) = child.lock().await.take() {
                let _ = proc.kill().await;
                let _ = proc.wait().await;
            }
            running.store(false, Ordering::SeqCst);
            Ok(())
        })
    }

    fn wipe(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let child = self.child.clone();
        let running = self.running.clone();
        let dd = self.data_dir.clone();
        Box::pin(async move {
            if let Some(mut proc) = child.lock().await.take() {
                let _ = proc.kill().await;
                let _ = proc.wait().await;
            }
            running.store(false, Ordering::SeqCst);
            if let Some(dir) = dd.lock().await.take() {
                if dir.exists() {
                    std::fs::remove_dir_all(&dir)
                        .map_err(|e| format!("Failed to remove data directory: {}", e))?;
                }
            }
            Ok(())
        })
    }

    fn execute_raw(&self, query: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let conn_info = self.conn_info.clone();
        Box::pin(async move {
            let info = conn_info.lock().await;
            let (host, port) = info.as_ref().ok_or("PostgreSQL is not running")?;
            let conn_str = format!(
                "host={} port={} user=postgres dbname=postgres connect_timeout=5",
                host, port
            );
            drop(info);

            let (client, connection) = tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                .await
                .map_err(|e| format!("Connection error: {}", e))?;
            tokio::spawn(async move { connection.await });

            let trimmed = query.trim().to_uppercase();
            let is_select = trimmed.starts_with("SELECT")
                || trimmed.starts_with("WITH")
                || trimmed.starts_with("SHOW")
                || trimmed.starts_with("PRAGMA")
                || trimmed.starts_with("EXPLAIN")
                || trimmed.starts_with("DESCRIBE");

            if is_select {
                match client.prepare(&query).await {
                    Ok(stmt) => {
                        let cols: Vec<String> = stmt.columns().iter().map(|c| c.name().to_string()).collect();
                        match client.query(&stmt, &[]).await {
                            Ok(rows) => {
                                let json_rows: Vec<serde_json::Value> = rows.iter().map(|row| {
                                    let mut map = serde_json::Map::new();
                                    for (i, col) in cols.iter().enumerate() {
                                        let val: Option<&str> = row.try_get::<_, Option<&str>>(i).unwrap_or(None);
                                        map.insert(col.clone(), serde_json::Value::String(val.unwrap_or("NULL").to_string()));
                                    }
                                    serde_json::Value::Object(map)
                                }).collect();
                                let row_count = json_rows.len();
                                Ok(serde_json::json!({"command": "SELECT", "rowCount": row_count, "columns": cols, "rows": json_rows}).to_string())
                            }
                            Err(e) => Err(format!("Query error: {}", e)),
                        }
                    }
                    Err(e) => Err(format!("Query error: {}", e)),
                }
            } else {
                client.batch_execute(&query)
                    .await
                    .map_err(|e| format!("Query error: {}", e))?;
                let tag = if trimmed.starts_with("INSERT") { "INSERT" }
                    else if trimmed.starts_with("CREATE") { "CREATE TABLE" }
                    else { "OK" };
                Ok(serde_json::json!({"command": tag, "rowCount": 0, "columns": [], "rows": []}).to_string())
            }
        })
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
