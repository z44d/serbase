use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use super::{DatabaseEngine, LogCallback};

#[derive(Clone)]
enum RedisValue {
    String(String),
    Hash(HashMap<String, String>),
    List(Vec<String>),
    Set(Vec<String>),
    ZSet(HashMap<String, f64>),
}

#[derive(Clone)]
struct RedisEntry {
    value: RedisValue,
    expiry: Option<Instant>,
}

pub struct RedisEngine {
    store: Arc<Mutex<HashMap<String, RedisEntry>>>,
    running: Arc<AtomicBool>,
    stop_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    password: Arc<Mutex<Option<String>>>,
    authenticated_clients: Arc<Mutex<HashMap<String, bool>>>,
}

impl RedisEngine {
    pub fn new(password: Option<String>) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            stop_tx: Arc::new(Mutex::new(None)),
            password: Arc::new(Mutex::new(password)),
            authenticated_clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn handle_connection(
        stream: &mut TcpStream,
        store: Arc<Mutex<HashMap<String, RedisEntry>>>,
        password: Arc<Mutex<Option<String>>>,
        authenticated_clients: Arc<Mutex<HashMap<String, bool>>>,
        on_debug: LogCallback,
        peer: String,
    ) {
        let mut buf = vec![0u8; 65536];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = &buf[..n];
                    match Self::parse_resp(data) {
                        Ok(commands) => {
                            for cmd in commands {
                                if !Self::check_auth(&cmd, &password, &authenticated_clients, &peer).await {
                                    let err = Self::encode_error("NOAUTH Authentication required.");
                                    let _ = stream.write_all(&err).await;
                                    continue;
                                }
                                let response = Self::execute_command(&cmd, &store, &password, &authenticated_clients, &peer, &on_debug).await;
                                on_debug(format!("<< {}", String::from_utf8_lossy(&response)));
                                if let Err(e) = stream.write_all(&response).await {
                                    log::error!("Redis write error: {}", e);
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let err = Self::encode_error(&format!("ERR {}", e));
                            let _ = stream.write_all(&err).await;
                        }
                    }
                }
                Err(e) => {
                    log::error!("Redis read error: {}", e);
                    break;
                }
            }
        }
    }

    async fn check_auth(
        cmd: &[String],
        password: &Arc<Mutex<Option<String>>>,
        authenticated_clients: &Arc<Mutex<HashMap<String, bool>>>,
        peer: &str,
    ) -> bool {
        let pass = password.lock().await;
        if pass.is_none() { return true; }
        if cmd.is_empty() { return false; }
        if cmd[0].to_uppercase() == "AUTH" { return true; }
        let auth = authenticated_clients.lock().await;
        *auth.get(peer).unwrap_or(&false)
    }

    fn parse_resp(data: &[u8]) -> Result<Vec<Vec<String>>, String> {
        let text = std::str::from_utf8(data).map_err(|e| format!("UTF-8 error: {}", e))?;
        let mut commands = Vec::new();
        let mut lines: Vec<&str> = text.split("\r\n").collect();
        if lines.last() == Some(&"") { lines.pop(); }

        let mut i = 0;
        while i < lines.len() {
            if lines[i].starts_with('*') {
                let count: usize = lines[i][1..].parse().map_err(|_| "Invalid array length")?;
                i += 1;
                let mut args = Vec::new();
                for _ in 0..count {
                    while i < lines.len() && (lines[i].is_empty() || lines[i].starts_with('$')) {
                        if lines[i].starts_with('$') {
                            i += 1;
                        } else {
                            i += 1;
                            continue;
                        }
                    }
                    if i < lines.len() && !lines[i].is_empty() {
                        args.push(lines[i].to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                if !args.is_empty() { commands.push(args); }
            } else if !lines[i].is_empty() && !lines[i].starts_with('$') {
                let parts: Vec<String> = lines[i].split(' ').map(|s| s.to_string()).collect();
                if !parts.is_empty() { commands.push(parts); }
                i += 1;
            } else {
                i += 1;
            }
        }
        Ok(commands)
    }

    async fn execute_command(
        cmd: &[String],
        store: &Arc<Mutex<HashMap<String, RedisEntry>>>,
        password: &Arc<Mutex<Option<String>>>,
        authenticated_clients: &Arc<Mutex<HashMap<String, bool>>>,
        peer: &str,
        on_debug: &LogCallback,
    ) -> Vec<u8> {
        if cmd.is_empty() { return Self::encode_error("ERR empty command"); }
        let op = cmd[0].to_uppercase();
        let args = &cmd[1..];
        on_debug(format!(">> {} {:?}", op, args));

        if op == "AUTH" {
            let pass = password.lock().await;
            if let Some(expected) = pass.as_ref() {
                if args.first().map(|s| s.as_str()) == Some(expected.as_str()) {
                    authenticated_clients.lock().await.insert(peer.to_string(), true);
                    return Self::encode_simple_string("OK");
                } else {
                    return Self::encode_error("ERR invalid password");
                }
            }
            return Self::encode_error("ERR AUTH not configured");
        }

        match op.as_str() {
            "PING" => Self::encode_simple_string("PONG"),
            "ECHO" => Self::encode_bulk_string(args.first().map(|s| s.as_str())),
            "SET" => Self::cmd_set(store, args).await,
            "GET" => Self::cmd_get(store, args).await,
            "DEL" => Self::cmd_del(store, args).await,
            "EXISTS" => Self::cmd_exists(store, args).await,
            "KEYS" => Self::cmd_keys(store, args).await,
            "EXPIRE" => Self::cmd_expire(store, args).await,
            "TTL" => Self::cmd_ttl(store, args).await,
            "TYPE" => Self::cmd_type(store, args).await,
            "HSET" => Self::cmd_hset(store, args).await,
            "HGET" => Self::cmd_hget(store, args).await,
            "HGETALL" => Self::cmd_hgetall(store, args).await,
            "HDEL" => Self::cmd_hdel(store, args).await,
            "LPUSH" => Self::cmd_lpush(store, args).await,
            "RPUSH" => Self::cmd_rpush(store, args).await,
            "LPOP" => Self::cmd_lpop(store, args).await,
            "RPOP" => Self::cmd_rpop(store, args).await,
            "LLEN" => Self::cmd_llen(store, args).await,
            "LRANGE" => Self::cmd_lrange(store, args).await,
            "SADD" => Self::cmd_sadd(store, args).await,
            "SMEMBERS" => Self::cmd_smembers(store, args).await,
            "SISMEMBER" => Self::cmd_sismember(store, args).await,
            "SCARD" => Self::cmd_scard(store, args).await,
            "SREM" => Self::cmd_srem(store, args).await,
            "ZADD" => Self::cmd_zadd(store, args).await,
            "ZRANGE" => Self::cmd_zrange(store, args).await,
            "ZCARD" => Self::cmd_zcard(store, args).await,
            "ZSCORE" => Self::cmd_zscore(store, args).await,
            "ZREM" => Self::cmd_zrem(store, args).await,
            "INCR" => Self::cmd_incr(store, args).await,
            "DECR" => Self::cmd_decr(store, args).await,
            "INCRBY" => Self::cmd_incrby(store, args).await,
            "DECRBY" => Self::cmd_decrby(store, args).await,
            "FLUSHALL" | "FLUSHDB" => { store.lock().await.clear(); Self::encode_simple_string("OK") }
            "DBSIZE" => Self::encode_integer(store.lock().await.len() as i64),
            "RANDOMKEY" => Self::cmd_randomkey(store).await,
            "RENAME" => Self::cmd_rename(store, args).await,
            "MGET" => Self::cmd_mget(store, args).await,
            "MSET" => Self::cmd_mset(store, args).await,
            "STRLEN" => Self::cmd_strlen(store, args).await,
            "APPEND" => Self::cmd_append(store, args).await,
            "GETRANGE" => Self::cmd_getrange(store, args).await,
            "SETEX" => Self::cmd_setex(store, args).await,
            "PSETEX" => Self::cmd_psetex(store, args).await,
            "CLIENT" | "SELECT" => Self::encode_simple_string("OK"),
            "COMMAND" => Self::encode_array(&[]),
            "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" => Self::encode_simple_string("OK"),
            "HELLO" => {
                let proto = args.first().and_then(|s| s.parse::<i32>().ok()).unwrap_or(2);
                Self::encode_hello_response(proto)
            }
            "INFO" => {
                let info = "# Server\r\nredis_version:7.2.0\r\ntcp_port:6379\r\n\r\n# Keyspace\r\ndb0:keys=0,expires=0".to_string();
                Self::encode_bulk_string(Some(&info))
            }
            _ => Self::encode_error(&format!("ERR unknown command '{}'", op)),
        }
    }

    async fn get_entry(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, key: &str) -> Option<RedisEntry> {
        let mut map = store.lock().await;
        if let Some(entry) = map.get(key) {
            if let Some(expiry) = entry.expiry {
                if Instant::now() >= expiry { map.remove(key); return None; }
            }
            return Some(entry.clone());
        }
        None
    }

    async fn cmd_set(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'SET' command"); }
        let mut expiry_ms: Option<u64> = None;
        let mut i = 2;
        while i < args.len() {
            match args[i].to_uppercase().as_str() {
                "EX" => { if i + 1 < args.len() { expiry_ms = args[i+1].parse::<u64>().ok().map(|s| s * 1000); i += 2; } else { i += 1; } }
                "PX" => { if i + 1 < args.len() { expiry_ms = args[i+1].parse::<u64>().ok(); i += 2; } else { i += 1; } }
                _ => i += 1,
            }
        }
        let mut map = store.lock().await;
        map.insert(args[0].clone(), RedisEntry {
            value: RedisValue::String(args[1].clone()),
            expiry: expiry_ms.map(|ms| Instant::now() + Duration::from_millis(ms)),
        });
        Self::encode_simple_string("OK")
    }

    async fn cmd_get(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'GET' command"); }
        match Self::get_entry(store, &args[0]).await {
            Some(e) => match e.value { RedisValue::String(s) => Self::encode_bulk_string(Some(&s)), _ => Self::encode_error("WRONGTYPE") },
            None => Self::encode_null(),
        }
    }

    async fn cmd_del(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        let mut map = store.lock().await;
        let count = args.iter().filter(|k| map.remove(*k).is_some()).count();
        Self::encode_integer(count as i64)
    }

    async fn cmd_exists(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        let mut count = 0i64;
        for key in args { if Self::get_entry(store, key).await.is_some() { count += 1; } }
        Self::encode_integer(count)
    }

    async fn cmd_keys(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        let pattern = args.first().map(|s| s.as_str()).unwrap_or("*");
        let map = store.lock().await;
        let keys: Vec<String> = map.keys().filter(|k| *k == pattern || pattern == "*").cloned().collect();
        let encoded: Vec<Vec<u8>> = keys.iter().map(|k| Self::encode_bulk_string(Some(k))).collect();
        Self::encode_array(&encoded)
    }

    async fn cmd_expire(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'EXPIRE' command"); }
        let seconds: u64 = match args[1].parse() { Ok(s) => s, Err(_) => return Self::encode_error("ERR value is not an integer") };
        let mut map = store.lock().await;
        match map.get_mut(&args[0]) {
            Some(e) => { e.expiry = Some(Instant::now() + Duration::from_secs(seconds)); Self::encode_integer(1) }
            None => Self::encode_integer(0),
        }
    }

    async fn cmd_ttl(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'TTL' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match e.expiry { Some(exp) => Self::encode_integer(exp.saturating_duration_since(Instant::now()).as_secs() as i64), None => Self::encode_integer(-1) },
            None => Self::encode_integer(-2),
        }
    }

    async fn cmd_type(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'TYPE' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => Self::encode_simple_string(match e.value { RedisValue::String(_) => "string", RedisValue::Hash(_) => "hash", RedisValue::List(_) => "list", RedisValue::Set(_) => "set", RedisValue::ZSet(_) => "zset" }),
            None => Self::encode_simple_string("none"),
        }
    }

    async fn cmd_hset(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'HSET' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::Hash(HashMap::new()), expiry: None });
        match &mut entry.value {
            RedisValue::Hash(h) => {
                let mut count = 0i64;
                let mut i = 1;
                while i + 1 < args.len() { if h.insert(args[i].clone(), args[i+1].clone()).is_none() { count += 1; } i += 2; }
                Self::encode_integer(count)
            }
            _ => Self::encode_error("WRONGTYPE"),
        }
    }

    async fn cmd_hget(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'HGET' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value { RedisValue::Hash(h) => Self::encode_bulk_string(h.get(&args[1]).map(|s| s.as_str())), _ => Self::encode_error("WRONGTYPE") },
            None => Self::encode_null(),
        }
    }

    async fn cmd_hgetall(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'HGETALL' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::Hash(h) => {
                    let mut items = Vec::new();
                    for (k, v) in h { items.push(Self::encode_bulk_string(Some(k))); items.push(Self::encode_bulk_string(Some(v))); }
                    Self::encode_array(&items)
                }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_array(&[]),
        }
    }

    async fn cmd_hdel(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'HDEL' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::Hash(h) => {
                    let mut h = h.clone();
                    let count = args[1..].iter().filter(|f| h.remove(f.as_str()).is_some()).count() as i64;
                    Self::encode_integer(count)
                }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_integer(0),
        }
    }

    async fn cmd_lpush(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'LPUSH' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::List(Vec::new()), expiry: None });
        match &mut entry.value {
            RedisValue::List(list) => { for v in args[1..].iter().rev() { list.insert(0, v.clone()); } Self::encode_integer(list.len() as i64) }
            _ => Self::encode_error("WRONGTYPE"),
        }
    }

    async fn cmd_rpush(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'RPUSH' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::List(Vec::new()), expiry: None });
        match &mut entry.value { RedisValue::List(list) => { list.extend_from_slice(&args[1..]); Self::encode_integer(list.len() as i64) } _ => Self::encode_error("WRONGTYPE") }
    }

    async fn cmd_lpop(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'LPOP' command"); }
        let mut map = store.lock().await;
        match map.get_mut(&args[0]) {
            Some(e) => match &mut e.value { RedisValue::List(list) => Self::encode_bulk_string(list.first().cloned().as_deref()), _ => Self::encode_error("WRONGTYPE") },
            None => Self::encode_null(),
        }
    }

    async fn cmd_rpop(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'RPOP' command"); }
        let mut map = store.lock().await;
        match map.get_mut(&args[0]) {
            Some(e) => match &mut e.value { RedisValue::List(list) => Self::encode_bulk_string(list.pop().as_deref()), _ => Self::encode_error("WRONGTYPE") },
            None => Self::encode_null(),
        }
    }

    async fn cmd_llen(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'LLEN' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value { RedisValue::List(list) => Self::encode_integer(list.len() as i64), _ => Self::encode_error("WRONGTYPE") },
            None => Self::encode_integer(0),
        }
    }

    async fn cmd_lrange(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'LRANGE' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::List(list) => {
                    let start: isize = args[1].parse().unwrap_or(0);
                    let stop: isize = args[2].parse().unwrap_or(-1);
                    let len = list.len() as isize;
                    let s = if start < 0 { (len + start).max(0) } else { start.min(len - 1).max(0) };
                    let e2 = if stop < 0 { (len + stop).max(0) } else { stop.min(len - 1).max(0) };
                    let items: Vec<Vec<u8>> = list[s as usize..=e2 as usize].iter().map(|v| Self::encode_bulk_string(Some(v))).collect();
                    Self::encode_array(&items)
                }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_array(&[]),
        }
    }

    async fn cmd_sadd(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'SADD' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::Set(Vec::new()), expiry: None });
        match &mut entry.value {
            RedisValue::Set(set) => { let mut c = 0; for v in &args[1..] { if !set.contains(v) { set.push(v.clone()); c += 1; } } Self::encode_integer(c) }
            _ => Self::encode_error("WRONGTYPE"),
        }
    }

    async fn cmd_smembers(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'SMEMBERS' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::Set(set) => { let items: Vec<Vec<u8>> = set.iter().map(|v| Self::encode_bulk_string(Some(v))).collect(); Self::encode_array(&items) }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_array(&[]),
        }
    }

    async fn cmd_sismember(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'SISMEMBER' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) { Some(e) => match &e.value { RedisValue::Set(set) => Self::encode_integer(if set.contains(&args[1]) { 1 } else { 0 }), _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_scard(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'SCARD' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) { Some(e) => match &e.value { RedisValue::Set(set) => Self::encode_integer(set.len() as i64), _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_srem(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'SREM' command"); }
        let mut map = store.lock().await;
        match map.get_mut(&args[0]) { Some(e) => match &mut e.value { RedisValue::Set(set) => { let b = set.len(); set.retain(|x| !args[1..].contains(x)); Self::encode_integer((b - set.len()) as i64) } _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_zadd(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'ZADD' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::ZSet(HashMap::new()), expiry: None });
        match &mut entry.value {
            RedisValue::ZSet(zs) => { let mut c = 0; let mut i = 1; while i + 1 < args.len() { if let Ok(score) = args[i+1].parse::<f64>() { if zs.insert(args[i].clone(), score).is_none() { c += 1; } } i += 2; } Self::encode_integer(c) }
            _ => Self::encode_error("WRONGTYPE"),
        }
    }

    async fn cmd_zrange(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'ZRANGE' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::ZSet(zs) => {
                    let mut vec: Vec<(&String, &f64)> = zs.iter().collect();
                    vec.sort_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));
                    let start: isize = args[1].parse().unwrap_or(0);
                    let stop: isize = args[2].parse().unwrap_or(-1);
                    let len = vec.len() as isize;
                    let s = if start < 0 { (len + start).max(0) } else { start.min(len - 1).max(0) };
                    let e2 = if stop < 0 { (len + stop).max(0) } else { stop.min(len - 1).max(0) };
                    let with_scores = args.iter().any(|a| a.to_uppercase() == "WITHSCORES");
                    let mut items = Vec::new();
                    for idx in s as usize..=e2 as usize { if idx < vec.len() { items.push(Self::encode_bulk_string(Some(vec[idx].0))); if with_scores { items.push(Self::encode_bulk_string(Some(&vec[idx].1.to_string()))); } } }
                    Self::encode_array(&items)
                }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_array(&[]),
        }
    }

    async fn cmd_zcard(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'ZCARD' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) { Some(e) => match &e.value { RedisValue::ZSet(zs) => Self::encode_integer(zs.len() as i64), _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_zscore(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'ZSCORE' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::ZSet(zs) => match zs.get(&args[1]) { Some(s) => Self::encode_bulk_string(Some(&s.to_string())), None => Self::encode_null() },
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_null(),
        }
    }

    async fn cmd_zrem(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'ZREM' command"); }
        let mut map = store.lock().await;
        match map.get_mut(&args[0]) { Some(e) => match &mut e.value { RedisValue::ZSet(zs) => { let mut c = 0; for v in &args[1..] { if zs.remove(v).is_some() { c += 1; } } Self::encode_integer(c) } _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_incr_base(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, key: &str, by: i64) -> Vec<u8> {
        let mut map = store.lock().await;
        let entry = map.entry(key.to_string()).or_insert(RedisEntry { value: RedisValue::String("0".into()), expiry: None });
        if let RedisValue::String(s) = &entry.value {
            let n: i64 = s.parse().unwrap_or(0);
            let new = n + by;
            entry.value = RedisValue::String(new.to_string());
            Self::encode_integer(new)
        } else { Self::encode_error("WRONGTYPE") }
    }

    async fn cmd_incr(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'INCR' command"); }
        Self::cmd_incr_base(store, &args[0], 1).await
    }

    async fn cmd_decr(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'DECR' command"); }
        Self::cmd_incr_base(store, &args[0], -1).await
    }

    async fn cmd_incrby(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'INCRBY' command"); }
        let by: i64 = args[1].parse().unwrap_or(0);
        Self::cmd_incr_base(store, &args[0], by).await
    }

    async fn cmd_decrby(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'DECRBY' command"); }
        let by: i64 = args[1].parse().unwrap_or(0);
        Self::cmd_incr_base(store, &args[0], -by).await
    }

    async fn cmd_randomkey(store: &Arc<Mutex<HashMap<String, RedisEntry>>>) -> Vec<u8> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let map = store.lock().await;
        let keys: Vec<&String> = map.keys().collect();
        if keys.is_empty() { return Self::encode_null(); }
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let idx = nanos as usize % keys.len();
        Self::encode_bulk_string(Some(keys[idx]))
    }

    async fn cmd_rename(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'RENAME' command"); }
        let mut map = store.lock().await;
        match map.remove(&args[0]) { Some(entry) => { map.insert(args[1].clone(), entry); Self::encode_simple_string("OK") } None => Self::encode_error("ERR no such key") }
    }

    async fn cmd_mget(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        let map = store.lock().await;
        let items: Vec<Vec<u8>> = args.iter().map(|k| match map.get(k) { Some(e) => match &e.value { RedisValue::String(s) => Self::encode_bulk_string(Some(s)), _ => Self::encode_null() }, None => Self::encode_null() }).collect();
        Self::encode_array(&items)
    }

    async fn cmd_mset(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 || args.len() % 2 != 0 { return Self::encode_error("ERR wrong number of arguments for 'MSET' command"); }
        let mut map = store.lock().await;
        let mut i = 0;
        while i + 1 < args.len() { map.insert(args[i].clone(), RedisEntry { value: RedisValue::String(args[i+1].clone()), expiry: None }); i += 2; }
        Self::encode_simple_string("OK")
    }

    async fn cmd_strlen(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.is_empty() { return Self::encode_error("ERR wrong number of arguments for 'STRLEN' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) { Some(e) => match &e.value { RedisValue::String(s) => Self::encode_integer(s.len() as i64), _ => Self::encode_error("WRONGTYPE") }, None => Self::encode_integer(0) }
    }

    async fn cmd_append(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 2 { return Self::encode_error("ERR wrong number of arguments for 'APPEND' command"); }
        let mut map = store.lock().await;
        let entry = map.entry(args[0].clone()).or_insert(RedisEntry { value: RedisValue::String(String::new()), expiry: None });
        if let RedisValue::String(s) = &mut entry.value { s.push_str(&args[1]); Self::encode_integer(s.len() as i64) } else { Self::encode_error("WRONGTYPE") }
    }

    async fn cmd_getrange(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'GETRANGE' command"); }
        let map = store.lock().await;
        match map.get(&args[0]) {
            Some(e) => match &e.value {
                RedisValue::String(s) => {
                    let start: isize = args[1].parse().unwrap_or(0);
                    let end: isize = args[2].parse().unwrap_or(-1);
                    let len = s.len() as isize;
                    let si = if start < 0 { (len + start).max(0) } else { start.min(len) };
                    let ei = if end < 0 { (len + end).max(0) } else { end.min(len - 1) };
                    if si <= ei { Self::encode_bulk_string(Some(&s[si as usize..=ei as usize])) } else { Self::encode_bulk_string(Some("")) }
                }
                _ => Self::encode_error("WRONGTYPE"),
            },
            None => Self::encode_bulk_string(Some("")),
        }
    }

    async fn cmd_setex(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'SETEX' command"); }
        let seconds: u64 = args[1].parse().unwrap_or(0);
        let mut map = store.lock().await;
        map.insert(args[0].clone(), RedisEntry { value: RedisValue::String(args[2].clone()), expiry: Some(Instant::now() + Duration::from_secs(seconds)) });
        Self::encode_simple_string("OK")
    }

    async fn cmd_psetex(store: &Arc<Mutex<HashMap<String, RedisEntry>>>, args: &[String]) -> Vec<u8> {
        if args.len() < 3 { return Self::encode_error("ERR wrong number of arguments for 'PSETEX' command"); }
        let ms: u64 = args[1].parse().unwrap_or(0);
        let mut map = store.lock().await;
        map.insert(args[0].clone(), RedisEntry { value: RedisValue::String(args[2].clone()), expiry: Some(Instant::now() + Duration::from_millis(ms)) });
        Self::encode_simple_string("OK")
    }

    fn encode_simple_string(s: &str) -> Vec<u8> { format!("+{}\r\n", s).into_bytes() }
    fn encode_error(s: &str) -> Vec<u8> { format!("-{}\r\n", s).into_bytes() }
    fn encode_integer(n: i64) -> Vec<u8> { format!(":{}\r\n", n).into_bytes() }
    fn encode_bulk_string(s: Option<&str>) -> Vec<u8> {
        match s {
            Some(val) => { let bytes = val.as_bytes(); let mut r = format!("${}\r\n", bytes.len()).into_bytes(); r.extend_from_slice(bytes); r.extend_from_slice(b"\r\n"); r }
            None => b"$-1\r\n".to_vec(),
        }
    }
    fn encode_null() -> Vec<u8> { b"$-1\r\n".to_vec() }
    fn encode_array(items: &[Vec<u8>]) -> Vec<u8> {
        let header = format!("*{}\r\n", items.len());
        let mut result = header.into_bytes();
        for item in items { result.extend_from_slice(item); }
        result
    }
    fn encode_hello_response(proto: i32) -> Vec<u8> {
        let entries: [(&str, Vec<u8>); 7] = [
            ("server", Self::encode_bulk_string(Some("redis"))),
            ("version", Self::encode_bulk_string(Some("7.2.0"))),
            ("proto", Self::encode_integer(proto as i64)),
            ("id", Self::encode_integer(1)),
            ("mode", Self::encode_bulk_string(Some("standalone"))),
            ("role", Self::encode_bulk_string(Some("master"))),
            ("modules", Self::encode_array(&[])),
        ];
        if proto >= 3 {
            let mut result = format!("%{}\r\n", entries.len()).into_bytes();
            for (key, val) in &entries {
                result.extend_from_slice(&Self::encode_bulk_string(Some(key)));
                result.extend_from_slice(val);
            }
            result
        } else {
            let mut items = Vec::with_capacity(entries.len() * 2);
            for (key, val) in &entries {
                items.push(Self::encode_bulk_string(Some(key)));
                items.push(val.clone());
            }
            Self::encode_array(&items)
        }
    }

    fn resp_to_query_result(resp: &[u8], command: &str) -> String {
        let text = String::from_utf8_lossy(resp);
        if text.starts_with('-') {
            let err = text[1..].trim_end_matches("\r\n");
            return serde_json::json!({"error": err, "command": command, "rowCount": 0, "columns": [], "rows": []}).to_string();
        }
        if text.starts_with('+') {
            let val = text[1..].trim_end_matches("\r\n");
            return serde_json::json!({"command": command, "rowCount": 1, "columns": ["result"], "rows": [{"result": val}]}).to_string();
        }
        if text.starts_with(':') {
            let val = text[1..].trim_end_matches("\r\n");
            return serde_json::json!({"command": command, "rowCount": 1, "columns": ["result"], "rows": [{"result": val}]}).to_string();
        }
        if text.starts_with("$-1") {
            return serde_json::json!({"command": command, "rowCount": 0, "columns": ["value"], "rows": []}).to_string();
        }
        if text.starts_with('$') {
            let parts: Vec<&str> = text.splitn(2, "\r\n").collect();
            if parts.len() > 1 {
                let val = parts[1].trim_end_matches("\r\n");
                return serde_json::json!({"command": command, "rowCount": 1, "columns": ["value"], "rows": [{"value": val}]}).to_string();
            }
        }
        if text.starts_with('*') {
            let header_end = text.find("\r\n").unwrap_or(text.len());
            let count_str = &text[1..header_end];
            let count: usize = count_str.parse().unwrap_or(0);
            let mut values = Vec::new();
            let mut pos = header_end + 2;
            for _ in 0..count {
                if pos >= text.len() { break; }
                if text[pos..].starts_with("$-1\r\n") {
                    values.push(serde_json::Value::Null);
                    pos += 5;
                } else if text[pos..].starts_with('$') {
                    let rest = &text[pos..];
                    let len_end = rest.find("\r\n").unwrap_or(rest.len());
                    let val_start = pos + len_end + 2;
                    let val_end = text[val_start..].find("\r\n").map(|e| val_start + e).unwrap_or(text.len());
                    let val = &text[val_start..val_end];
                    values.push(serde_json::Value::String(val.to_string()));
                    pos = val_end + 2;
                } else if text[pos..].starts_with(':') {
                    let end = text[pos..].find("\r\n").map(|e| pos + e).unwrap_or(text.len());
                    values.push(serde_json::json!(text[pos+1..end]));
                    pos = end + 2;
                } else {
                    let end = text[pos..].find("\r\n").map(|e| pos + e).unwrap_or(text.len());
                    values.push(serde_json::Value::String(text[pos..end].to_string()));
                    pos = end + 2;
                }
            }
            let rows: Vec<serde_json::Value> = values.into_iter().map(|v| serde_json::json!({"value": v})).collect();
            return serde_json::json!({"command": command, "rowCount": rows.len(), "columns": ["value"], "rows": rows}).to_string();
        }
        serde_json::json!({"command": command, "rowCount": 0, "columns": [], "rows": []}).to_string()
    }
}

impl DatabaseEngine for RedisEngine {
    fn start(&self, host: String, port: u16, on_log: LogCallback, on_debug: LogCallback) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let store = self.store.clone();
        let running = self.running.clone();
        let stop_tx = self.stop_tx.clone();
        let password = self.password.clone();
        let authenticated_clients = self.authenticated_clients.clone();
        let addr = format!("{}:{}", host, port);
        Box::pin(async move {
            if running.load(Ordering::SeqCst) { return Err("Redis is already running".into()); }
            let listener = TcpListener::bind(&addr).await.map_err(|e| format!("Failed to bind Redis on {}: {}", addr, e))?;
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            *stop_tx.lock().await = Some(tx);
            running.store(true, Ordering::SeqCst);
            on_log(format!("Redis listening on {}", addr));
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        accept = listener.accept() => {
                            match accept {
                                Ok((mut stream, addr)) => {
                                    let peer = addr.to_string();
                                    on_debug(format!("New connection from {}", peer));
                                    let s = store.clone();
                                    let d = on_debug.clone();
                                    let p = password.clone();
                                    let a = authenticated_clients.clone();
                                    tokio::spawn(async move { RedisEngine::handle_connection(&mut stream, s, p, a, d, peer).await; });
                                }
                                Err(e) => { log::error!("Redis accept error: {}", e); break; }
                            }
                        }
                        _ = &mut rx => break,
                    }
                }
                running.store(false, Ordering::SeqCst);
                on_log("Redis stopped".into());
            });
            Ok(())
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let stop_tx = self.stop_tx.clone();
        Box::pin(async move { if let Some(tx) = stop_tx.lock().await.take() { let _ = tx.send(()); } Ok(()) })
    }

    fn wipe(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let store = self.store.clone();
        Box::pin(async move { store.lock().await.clear(); Ok(()) })
    }

    fn execute_raw(&self, query: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let store = self.store.clone();
        let password = self.password.clone();
        let authenticated_clients = self.authenticated_clients.clone();
        Box::pin(async move {
            let parts: Vec<String> = query.split_whitespace().map(|s| s.to_string()).collect();
            let command = parts.first().cloned().unwrap_or_default();
            let on_debug: LogCallback = Arc::new(|_| {});
            let response = Self::execute_command(&parts, &store, &password, &authenticated_clients, "admin", &on_debug).await;
            if response.starts_with(b"-") {
                let err = String::from_utf8_lossy(&response[1..]).trim_end_matches("\r\n").to_string();
                return Err(err);
            }
            Ok(Self::resp_to_query_result(&response, &command))
        })
    }

    fn is_running(&self) -> bool { self.running.load(Ordering::SeqCst) }
}
