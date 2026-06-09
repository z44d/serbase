use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use super::{DatabaseEngine, LogCallback};
use bson::{Bson, Document, doc};

struct Collection {
    documents: Vec<Document>,
}

pub struct MongoEngine {
    databases: Arc<Mutex<HashMap<String, HashMap<String, Collection>>>>,
    running: Arc<AtomicBool>,
    stop_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    password: Arc<Option<String>>,
}

impl MongoEngine {
    pub fn new(password: Option<String>) -> Self {
        Self {
            databases: Arc::new(Mutex::new({ let mut d = HashMap::new(); d.insert("test".into(), HashMap::new()); d })),
            running: Arc::new(AtomicBool::new(false)),
            stop_tx: Arc::new(Mutex::new(None)),
            password: Arc::new(password),
        }
    }

    fn json_val_to_bson(val: &serde_json::Value) -> Bson {
        match val {
            serde_json::Value::Null => Bson::Null,
            serde_json::Value::Bool(b) => Bson::Boolean(*b),
            serde_json::Value::Number(n) => n.as_i64().map(Bson::Int64).or_else(|| n.as_f64().map(Bson::Double)).unwrap_or(Bson::Null),
            serde_json::Value::String(s) => Bson::String(s.clone()),
            serde_json::Value::Array(arr) => Bson::Array(arr.iter().map(|v| Self::json_val_to_bson(v)).collect()),
            serde_json::Value::Object(obj) => {
                let mut doc = Document::new();
                for (k, v) in obj {
                    doc.insert(k.as_str(), Self::json_val_to_bson(v));
                }
                Bson::Document(doc)
            }
        }
    }

    fn bson_doc_to_json(doc: &Document) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (k, v) in doc {
            map.insert(k.clone(), Self::bson_to_json_val(v));
        }
        serde_json::Value::Object(map)
    }

    fn bson_to_json_val(bson: &Bson) -> serde_json::Value {
        match bson {
            Bson::String(s) => serde_json::Value::String(s.clone()),
            Bson::Int32(n) => serde_json::json!(n),
            Bson::Int64(n) => serde_json::json!(n),
            Bson::Double(n) => serde_json::json!(n),
            Bson::Boolean(b) => serde_json::Value::Bool(*b),
            Bson::Null => serde_json::Value::Null,
            Bson::Array(arr) => serde_json::Value::Array(arr.iter().map(|b| Self::bson_to_json_val(b)).collect()),
            Bson::Document(d) => Self::bson_doc_to_json(d),
            _ => serde_json::Value::String(format!("{:?}", bson)),
        }
    }

    // MongoDB OP_MSG wire protocol handler
    async fn handle_connection(
        stream: &mut TcpStream,
        databases: Arc<Mutex<HashMap<String, HashMap<String, Collection>>>>,
        password: Arc<Option<String>>,
        on_debug: LogCallback,
    ) {
        let mut buf = vec![0u8; 65536];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = &buf[..n];
                    on_debug(format!(">> Mongo request: {} bytes", n));

                    if data.len() < 16 {
                        on_debug("Message too short for header".into());
                        continue;
                    }

                    let msg_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                    let _request_id = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                    let _response_to = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
                    let op_code = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);

                    on_debug(format!("  opCode={}, msgLen={}", op_code, msg_len));

                    match op_code {
                        2013 => {
                            if data.len() < 20 { continue; }
                            let _flag_bits = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
                            let body_start = 20;

                            if body_start >= data.len() { continue; }

                            let section_kind = data[body_start];
                            let bson_start = body_start + 1;

                            if section_kind != 0 || bson_start >= data.len() {
                                on_debug(format!("Unhandled section kind: {}", section_kind));
                                continue;
                            }

                            let bson_data = &data[bson_start..msg_len.min(data.len())];
                            match bson::from_slice::<Document>(bson_data) {
                                Ok(doc) => {
                                    let response = Self::handle_bson_command(&doc, &databases, &password, &on_debug).await;
                                    let response_bson = match bson::to_vec(&response) {
                                        Ok(b) => b,
                                        Err(e) => {
                                            on_debug(format!("BSON encode error: {}", e));
                                            continue;
                                        }
                                    };

                                    let total_len = 16 + 4 + 1 + response_bson.len();
                                    let mut msg = Vec::with_capacity(total_len);
                                    msg.extend_from_slice(&(total_len as u32).to_le_bytes());
                                    msg.extend_from_slice(&[0,0,0,0]);
                                    msg.extend_from_slice(&_request_id.to_le_bytes());
                                    msg.extend_from_slice(&(2013u32).to_le_bytes());
                                    msg.extend_from_slice(&[0,0,0,0]);
                                    msg.push(0u8);
                                    msg.extend_from_slice(&response_bson);

                                    on_debug(format!("<< Response: {} bytes", total_len));
                                    if let Err(e) = stream.write_all(&msg).await {
                                        log::error!("Mongo write error: {}", e);
                                        return;
                                    }
                                }
                                Err(e) => {
                                    on_debug(format!("BSON parse error: {}", e));
                                }
                            }
                        }
                        2004 => {
                            if data.len() < 20 { continue; }
                            let _flags = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
                            let coll_start = 20;
                            let coll_end = data[coll_start..].iter().position(|&b| b == 0).map(|p| coll_start + p).unwrap_or(data.len());
                            let _full_coll_name = String::from_utf8_lossy(&data[coll_start..coll_end]);
                            on_debug(format!("  OP_QUERY: collection={}", _full_coll_name));
                            let skip_start = coll_end + 1;
                            if skip_start + 8 > data.len() { continue; }
                            let bson_start = skip_start + 8;
                            if bson_start >= data.len() { continue; }
                            let bson_data = &data[bson_start..msg_len.min(data.len())];
                            let response = match bson::from_slice::<Document>(bson_data) {
                                Ok(doc) => {
                                    let mut cmd_doc = doc.clone();
                                    if !cmd_doc.contains_key("$db") {
                                        let db = if _full_coll_name.contains(".$cmd") {
                                            _full_coll_name.split('.').next().unwrap_or("test")
                                        } else { "test" };
                                        cmd_doc.insert("$db", db);
                                    }
                                    Self::handle_bson_command(&cmd_doc, &databases, &password, &on_debug).await
                                }
                                Err(e) => {
                                    on_debug(format!("BSON parse error in OP_QUERY: {}", e));
                                    doc! { "ok": 0.0, "errmsg": format!("BSON parse error: {}", e) }
                                }
                            };
                            let response_bson = match bson::to_vec(&response) {
                                Ok(b) => b,
                                Err(e) => {
                                    on_debug(format!("BSON encode error: {}", e));
                                    continue;
                                }
                            };
                            let total_len = 16 + 4 + 8 + 4 + 4 + response_bson.len();
                            let mut msg = Vec::with_capacity(total_len);
                            msg.extend_from_slice(&(total_len as u32).to_le_bytes());
                            msg.extend_from_slice(&[0,0,0,0]);
                            msg.extend_from_slice(&_request_id.to_le_bytes());
                            msg.extend_from_slice(&(1u32).to_le_bytes());
                            msg.extend_from_slice(&[0,0,0,0]);
                            msg.extend_from_slice(&[0,0,0,0,0,0,0,0]);
                            msg.extend_from_slice(&[0,0,0,0]);
                            msg.extend_from_slice(&(1u32).to_le_bytes());
                            msg.extend_from_slice(&response_bson);
                            on_debug(format!("<< OP_REPLY: {} bytes", total_len));
                            if let Err(e) = stream.write_all(&msg).await {
                                log::error!("Mongo write error: {}", e);
                                return;
                            }
                        }
                        2012 => {
                            if data.len() < 25 {
                                on_debug("OP_COMPRESSED message too short".into());
                                continue;
                            }
                            let original_op_code = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
                            let uncompressed_size = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
                            let compressor_id = data[24];
                            let compressed_data = if data.len() > 25 { &data[25..msg_len.min(data.len())] } else { &[] };

                            on_debug(format!("  OP_COMPRESSED: originalOpCode={}, uncompressedSize={}, compressorId={}", original_op_code, uncompressed_size, compressor_id));

                            let decompressed = match compressor_id {
                                0 => compressed_data.to_vec(),
                                _ => {
                                    on_debug(format!("  Unsupported compressor: {}", compressor_id));
                                    continue;
                                }
                            };

                            if decompressed.len() < 16 {
                                on_debug("Decompressed message too short".into());
                                continue;
                            }

                            let inner_msg_len = u32::from_le_bytes([
                                decompressed[0], decompressed[1], decompressed[2], decompressed[3],
                            ]) as usize;
                            let _inner_request_id = u32::from_le_bytes([
                                decompressed[4], decompressed[5], decompressed[6], decompressed[7],
                            ]);
                            let _inner_response_to = u32::from_le_bytes([
                                decompressed[8], decompressed[9], decompressed[10], decompressed[11],
                            ]);

                            on_debug(format!("  Decompressed: opCode={}, msgLen={}", original_op_code, inner_msg_len));

                            match original_op_code {
                                2013 => {
                                    if decompressed.len() < 20 { continue; }
                                    let _flag_bits = u32::from_le_bytes([
                                        decompressed[16], decompressed[17], decompressed[18], decompressed[19],
                                    ]);
                                    let body_start = 20;
                                    if body_start >= decompressed.len() { continue; }
                                    let section_kind = decompressed[body_start];
                                    let bson_start = body_start + 1;
                                    if section_kind != 0 || bson_start >= decompressed.len() {
                                        on_debug(format!("Unhandled section kind in decompressed: {}", section_kind));
                                        continue;
                                    }
                                    let bson_data = &decompressed[bson_start..inner_msg_len.min(decompressed.len())];
                                    match bson::from_slice::<Document>(bson_data) {
                                        Ok(doc) => {
                                            let response = Self::handle_bson_command(&doc, &databases, &password, &on_debug).await;
                                            let response_bson = match bson::to_vec(&response) {
                                                Ok(b) => b,
                                                Err(e) => {
                                                    on_debug(format!("BSON encode error: {}", e));
                                                    continue;
                                                }
                                            };
                                            let total_len = 16 + 4 + 1 + response_bson.len();
                                            let mut msg = Vec::with_capacity(total_len);
                                            msg.extend_from_slice(&(total_len as u32).to_le_bytes());
                                            msg.extend_from_slice(&[0,0,0,0]);
                                            msg.extend_from_slice(&_inner_request_id.to_le_bytes());
                                            msg.extend_from_slice(&(2013u32).to_le_bytes());
                                            msg.extend_from_slice(&[0,0,0,0]);
                                            msg.push(0u8);
                                            msg.extend_from_slice(&response_bson);
                                            on_debug(format!("<< Response to compressed: {} bytes", total_len));
                                            if let Err(e) = stream.write_all(&msg).await {
                                                log::error!("Mongo write error: {}", e);
                                                return;
                                            }
                                        }
                                        Err(e) => {
                                            on_debug(format!("BSON parse error in decompressed: {}", e));
                                        }
                                    }
                                }
                                _ => {
                                    on_debug(format!("Unsupported original opCode: {}", original_op_code));
                                }
                            }
                        }
                        _ => {
                            on_debug(format!("Unsupported opCode: {}", op_code));
                        }
                    }
                }
                Err(e) => { log::error!("Mongo read error: {}", e); break; }
            }
        }
    }

    async fn handle_bson_command(
        doc: &Document,
        databases: &Arc<Mutex<HashMap<String, HashMap<String, Collection>>>>,
        password: &Arc<Option<String>>,
        on_debug: &LogCallback,
    ) -> Document {
        let cmd_name = doc.keys()
            .find(|k| !k.starts_with("$"))
            .cloned()
            .unwrap_or_default();

        let db = doc.get_str("$db").unwrap_or("test");

        on_debug(format!("Command: {} on db: {}", cmd_name, db));

        if cmd_name == "saslStart" || cmd_name == "saslContinue" {
            return Self::handle_auth(&cmd_name, doc, password);
        }

        let response = match cmd_name.as_str() {
            "isMaster" | "ismaster" | "hello" => {
                doc! {
                    "ismaster": true, "ok": 1.0,
                    "maxBsonObjectSize": 16777216_i32,
                    "maxMessageSizeBytes": 48000000_i32,
                    "maxWriteBatchSize": 100000_i32,
                    "minWireVersion": 0_i32,
                    "maxWireVersion": 21_i32,
                    "saslSupportedMechs": ["SCRAM-SHA-256", "SCRAM-SHA-1"],
                }
            }
            "ping" => doc! { "ok": 1.0 },
            "buildInfo" => doc! { "version": "7.0.0", "ok": 1.0 },
            "listDatabases" => {
                let dbs = databases.lock().await;
                let list: Vec<Bson> = dbs.keys().map(|k| Bson::Document(doc! {
                    "name": k.as_str(),
                    "sizeOnDisk": 1_i64,
                    "empty": false,
                })).collect();
                let size = list.len() as i64;
                doc! { "databases": Bson::Array(list), "totalSize": size, "ok": 1.0 }
            }
            "listCollections" => {
                let dbs = databases.lock().await;
                let colls: Vec<Bson> = match dbs.get(db) {
                    Some(c) => c.keys().map(|k| Bson::Document(doc! {
                        "name": k.as_str(),
                        "type": "collection",
                    })).collect(),
                    None => vec![],
                };
                doc! { "cursor": { "id": 0_i64, "firstBatch": Bson::Array(colls) }, "ok": 1.0 }
            }
            "find" => {
                let coll_name = doc.get_str("find").unwrap_or("unknown");
                let filter_doc = doc.get_document("filter").ok();
                let dbs = databases.lock().await;
                let docs: Vec<Bson> = match dbs.get(db).and_then(|c| c.get(coll_name)) {
                    Some(col) => col.documents.iter()
                        .filter(|d| filter_doc.map_or(true, |f| Self::matches_doc(d, f)))
                        .map(|d| Bson::Document(d.clone()))
                        .collect(),
                    None => vec![],
                };
                doc! { "cursor": { "id": 0_i64, "firstBatch": Bson::Array(docs), "ns": format!("{}.{}", db, coll_name) }, "ok": 1.0 }
            }
            "insert" => {
                let coll_name = doc.get_str("insert").unwrap_or("unknown");
                let docs_arr = doc.get_array("documents").ok();
                let mut dbs = databases.lock().await;
                let collections = dbs.entry(db.to_string()).or_insert(HashMap::new());
                let collection = collections.entry(coll_name.to_string()).or_insert(Collection { documents: Vec::new() });
                if let Some(arr) = docs_arr {
                    for bson_doc in arr {
                        if let Bson::Document(d) = bson_doc {
                            collection.documents.push(d.clone());
                        }
                    }
                }
                doc! { "n": docs_arr.map_or(0, |a| a.len()) as i64, "ok": 1.0 }
            }
            "delete" => {
                let coll_name = doc.get_str("delete").unwrap_or("unknown");
                let deletes = doc.get_array("deletes").ok();
                let mut dbs = databases.lock().await;
                let mut removed = 0usize;
                if let Some(collections) = dbs.get_mut(db) {
                    if let Some(collection) = collections.get_mut(coll_name) {
                        if let Some(arr) = deletes {
                            for del in arr {
                                if let Bson::Document(del_doc) = del {
                                    let filter = del_doc.get_document("filter").ok();
                                    let before = collection.documents.len();
                                    collection.documents.retain(|d| !filter.map_or(false, |f| Self::matches_doc(d, f)));
                                    removed += before - collection.documents.len();
                                }
                            }
                        }
                    }
                }
                doc! { "n": removed as i64, "ok": 1.0 }
            }
            "update" => {
                let coll_name = doc.get_str("update").unwrap_or("unknown");
                let updates = doc.get_array("updates").ok();
                let mut dbs = databases.lock().await;
                let mut modified = 0usize;
                if let Some(collections) = dbs.get_mut(db) {
                    if let Some(collection) = collections.get_mut(coll_name) {
                        if let Some(arr) = updates {
                            for up in arr {
                                if let Bson::Document(up_doc) = up {
                                    let filter = up_doc.get_document("filter").ok();
                                    let update_doc = up_doc.get_document("u").ok();
                                    for doc in &mut collection.documents {
                                        if filter.map_or(true, |f| Self::matches_doc(doc, f)) {
                                            if let Some(u) = update_doc {
                                                if let Some(set) = u.get_document("$set").ok() {
                                                    for (k, v) in set {
                                                        doc.insert(k.as_str(), v.clone());
                                                    }
                                                }
                                            }
                                            modified += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                doc! { "n": modified as i64, "nModified": modified as i64, "ok": 1.0 }
            }
            "create" => {
                let coll_name = doc.get_str("create").unwrap_or("unknown");
                let mut dbs = databases.lock().await;
                dbs.entry(db.to_string()).or_insert(HashMap::new()).entry(coll_name.to_string()).or_insert(Collection { documents: Vec::new() });
                doc! { "ok": 1.0 }
            }
            "drop" => {
                let coll_name = doc.get_str("drop").unwrap_or("");
                let mut dbs = databases.lock().await;
                if let Some(collections) = dbs.get_mut(db) { collections.remove(coll_name); }
                doc! { "ok": 1.0 }
            }
            "count" => {
                let coll_name = doc.get_str("count").unwrap_or("unknown");
                let filter_doc = doc.get_document("filter").ok();
                let dbs = databases.lock().await;
                let count = dbs.get(db).and_then(|c| c.get(coll_name)).map(|col| {
                    col.documents.iter().filter(|d| filter_doc.map_or(true, |f| Self::matches_doc(d, f))).count()
                }).unwrap_or(0);
                doc! { "n": count as i64, "ok": 1.0 }
            }
            "aggregate" => {
                let coll_name = doc.get_str("aggregate").unwrap_or("unknown");
                let dbs = databases.lock().await;
                let docs: Vec<Bson> = dbs.get(db).and_then(|c| c.get(coll_name))
                    .map(|col| col.documents.iter().map(|d| Bson::Document(d.clone())).collect())
                    .unwrap_or_default();
                doc! { "cursor": { "id": 0_i64, "firstBatch": Bson::Array(docs) }, "ok": 1.0 }
            }
            "getParameter" => doc! { "featureCompatibilityVersion": { "version": "7.0" }, "ok": 1.0 },
            "whatsmyuri" => doc! { "you": "127.0.0.1:27017", "ok": 1.0 },
            "getLastError" => doc! { "n": 0_i64, "connectionId": 1_i64, "ok": 1.0 },
            "replSetGetStatus" => doc! { "ok": 0.0, "errmsg": "not running with --replSet" },
            "listIndexes" => doc! { "cursor": { "id": 0_i64, "firstBatch": Bson::Array(vec![]) }, "ok": 1.0 },
            _ => doc! { "ok": 0.0, "errmsg": format!("no such command: '{}'", cmd_name) },
        };

        response
    }

    fn handle_auth(cmd_name: &str, doc: &Document, password: &Arc<Option<String>>) -> Document {
        if password.is_none() {
            return doc! { "ok": 1.0, "done": true };
        }
        match cmd_name {
            "saslStart" => {
                let _user = doc.get_str("user").unwrap_or("admin");
                let _mechanism = doc.get_str("mechanism").unwrap_or("SCRAM-SHA-256");
                let server_nonce = format!("{}", uuid::Uuid::new_v4());
                let salt = "c2VyYmVhc2U=";
                doc! {
                    "ok": 1.0,
                    "conversationId": 1_i32,
                    "done": false,
                    "payload": format!("r={},s={},i=4096", &server_nonce[..8], salt),
                    "serverNonce": &server_nonce[..16],
                    "salt": salt,
                    "iterationCount": 4096_i32,
                }
            }
            "saslContinue" => {
                doc! {
                    "ok": 1.0,
                    "conversationId": 1_i32,
                    "done": true,
                    "payload": "v=cmVhbGx5bm90dmVyaWZpZWQ=",
                }
            }
            _ => doc! { "ok": 0.0, "errmsg": "unknown auth command" },
        }
    }

    fn matches_doc(doc: &Document, filter: &Document) -> bool {
        for (key, cond) in filter {
            if key == "$and" {
                if let Bson::Array(arr) = cond {
                    if !arr.iter().all(|c| {
                        if let Bson::Document(d) = c { Self::matches_doc(doc, d) } else { false }
                    }) { return false; }
                }
                continue;
            }
            if key == "$or" {
                if let Bson::Array(arr) = cond {
                    if !arr.iter().any(|c| {
                        if let Bson::Document(d) = c { Self::matches_doc(doc, d) } else { false }
                    }) { return false; }
                }
                continue;
            }

            let doc_val = doc.get(key.as_str());

            match cond {
                Bson::Document(cobj) => {
                    for (op, val) in cobj {
                        let dv = doc_val.unwrap_or(&Bson::Null);
                        let ok = match op.as_str() {
                            "$eq" => Self::bson_eq(dv, val),
                            "$ne" => !Self::bson_eq(dv, val),
                            "$gt" => Self::bson_cmp(dv, val, |a, b| a > b),
                            "$gte" => Self::bson_cmp(dv, val, |a, b| a >= b),
                            "$lt" => Self::bson_cmp(dv, val, |a, b| a < b),
                            "$lte" => Self::bson_cmp(dv, val, |a, b| a <= b),
                            "$exists" => {
                                let exists = *dv != Bson::Null;
                                val.as_bool().unwrap_or(false) == exists
                            }
                            "$regex" => {
                                if let Bson::String(s) = dv {
                                    if let Bson::String(p) = val {
                                        s.contains(p.as_str())
                                    } else { false }
                                } else { false }
                            }
                            _ => true,
                        };
                        if !ok { return false; }
                    }
                }
                _ => {
                    if let Some(dv) = doc_val {
                        if !Self::bson_eq(dv, cond) { return false; }
                    } else { return false; }
                }
            }
        }
        true
    }

    fn bson_eq(a: &Bson, b: &Bson) -> bool {
        match (a, b) {
            (Bson::String(a), Bson::String(b)) => a == b,
            (Bson::Int32(a), Bson::Int64(b)) => *a as i64 == *b,
            (Bson::Int64(a), Bson::Int32(b)) => *a == *b as i64,
            (Bson::Int32(a), Bson::Int32(b)) => a == b,
            (Bson::Int64(a), Bson::Int64(b)) => a == b,
            (Bson::Double(a), Bson::Double(b)) => (*a - *b).abs() < 0.001,
            (Bson::Double(a), Bson::Int32(b)) => (*a - *b as f64).abs() < 0.001,
            (Bson::Double(a), Bson::Int64(b)) => (*a - *b as f64).abs() < 0.001,
            (Bson::Boolean(a), Bson::Boolean(b)) => a == b,
            (Bson::Null, Bson::Null) => true,
            _ => a == b,
        }
    }

    fn bson_cmp<F: Fn(f64, f64) -> bool>(a: &Bson, b: &Bson, cmp: F) -> bool {
        let a_num = match a { Bson::Int32(n) => *n as f64, Bson::Int64(n) => *n as f64, Bson::Double(n) => *n, _ => return false };
        let b_num = match b { Bson::Int32(n) => *n as f64, Bson::Int64(n) => *n as f64, Bson::Double(n) => *n, _ => return false };
        cmp(a_num, b_num)
    }
}

impl DatabaseEngine for MongoEngine {
    fn start(&self, host: String, port: u16, on_log: LogCallback, on_debug: LogCallback) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let databases = self.databases.clone(); let running = self.running.clone(); let stop_tx = self.stop_tx.clone();
        let password = self.password.clone();
        let addr = format!("{}:{}", host, port);
        Box::pin(async move {
            if running.load(Ordering::SeqCst) { return Err("MongoDB is already running".into()); }
            let listener = TcpListener::bind(&addr).await.map_err(|e| format!("Failed to bind MongoDB on {}: {}", addr, e))?;
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            *stop_tx.lock().await = Some(tx); running.store(true, Ordering::SeqCst);
            on_log(format!("MongoDB listening on {}", addr));
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        accept = listener.accept() => { match accept { Ok((mut stream, addr)) => { on_debug(format!("New mongo connection from {}", addr)); let d = databases.clone(); let dbg = on_debug.clone(); let p = password.clone(); tokio::spawn(async move { MongoEngine::handle_connection(&mut stream, d, p, dbg).await; }); } Err(e) => { log::error!("Mongo accept error: {}", e); break; } } }
                        _ = &mut rx => break,
                    }
                }
                running.store(false, Ordering::SeqCst); on_log("MongoDB stopped".into());
            });
            Ok(())
        })
    }
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let stop_tx = self.stop_tx.clone();
        Box::pin(async move { if let Some(tx) = stop_tx.lock().await.take() { let _ = tx.send(()); } Ok(()) })
    }
    fn wipe(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let databases = self.databases.clone();
        Box::pin(async move { let mut d = databases.lock().await; d.clear(); d.insert("test".into(), HashMap::new()); Ok(()) })
    }
    fn execute_raw(&self, query: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let databases = self.databases.clone();
        Box::pin(async move {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&query) {
                let empty = serde_json::Map::new();
                let obj = parsed.as_object().unwrap_or(&empty);
                let mut doc = Document::new();
                for (k, v) in obj {
                    doc.insert(k.as_str(), Self::json_val_to_bson(v));
                }
                let password = Arc::new(None);
                let on_debug: LogCallback = Arc::new(|_| {});
                let response = Self::handle_bson_command(&doc, &databases, &password, &on_debug).await;
                let json = Self::bson_doc_to_json(&response);

                let rows: Vec<serde_json::Value> = if let Some(cursor) = json.get("cursor").and_then(|c: &serde_json::Value| c.as_object()) {
                    if let Some(batch) = cursor.get("firstBatch").and_then(|b: &serde_json::Value| b.as_array()) {
                        batch.clone()
                    } else { vec![] }
                } else if json.get("ok").and_then(|o: &serde_json::Value| o.as_f64()) == Some(1.0) && json.as_object().map(|o| o.contains_key("n")).unwrap_or(false) {
                    vec![json.clone()]
                } else {
                    vec![json.clone()]
                };
                let cols: Vec<String> = if !rows.is_empty() {
                    rows[0].as_object().map(|o: &serde_json::Map<String, serde_json::Value>| o.keys().cloned().collect()).unwrap_or_default()
                } else { vec![] };
                let cmd_name = obj.keys().next().cloned().unwrap_or_else(|| "command".to_string());
                Ok(serde_json::json!({"command": cmd_name, "rowCount": rows.len(), "columns": cols, "rows": rows}).to_string())
            } else {
                Ok(serde_json::json!({"command": "help", "rowCount": 0, "columns": [], "rows": [], "message": "Use JSON format: {\"find\":\"collection\",\"filter\":{}}"}).to_string())
            }
        })
    }
    fn is_running(&self) -> bool { self.running.load(Ordering::SeqCst) }
}
