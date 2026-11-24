//! QA-only subset of the 0.6.1 desktop stores. Validation never rewrites the bytes.
use super::{Payload, MAX_FILE, MAX_FILES, MAX_TOTAL};
use serde::{de::DeserializeOwned, Deserialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT: &str = "learnharness-0.6.1-disposable-v1";
const GRAPH: &[&str] = &[
    "graph/format.json",
    "graph/nodes.jsonl",
    "graph/chains.jsonl",
    "graph/coverage.jsonl",
    "graph/positions.json",
];

pub fn supported_path(path: &str) -> bool {
    GRAPH.contains(&path)
        || path
            .strip_prefix("chats/")
            .and_then(|s| s.strip_suffix(".json"))
            .is_some_and(|s| generated_id(s, "chat_"))
}
fn generated_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix)
        .and_then(|s| s.split_once('_'))
        .is_some_and(|(stamp, nonce)| {
            !stamp.is_empty()
                && stamp
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && nonce.len() == 8
                && nonce.bytes().all(|c| c.is_ascii_hexdigit())
        })
}
// Every listed field is required unless made optional explicitly. Unknown fields
// (including future tool payloads, media and path-bearing records) fail decoding.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Chat {
    version: u32,
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    messages: Vec<Message>,
    #[serde(default)]
    kind: Kind,
    #[serde(default)]
    turns: Vec<Turn>,
    #[serde(default)]
    extractions: Vec<Extraction>,
    settings: Option<Settings>,
}

#[derive(Deserialize, Default)]
enum Kind {
    #[default]
    #[serde(skip)]
    Teach,
    #[serde(rename = "practice")]
    Practice,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Settings {
    model: Model,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Model {
    provider: String,
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Step {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    cache_write1h_tokens: u64,
    reasoning_tokens: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Turn {
    id: String,
    message_id: String,
    at: String,
    model: String,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Extraction {
    started_at: u64,
    at: String,
    duration_ms: u64,
    stats: Stats,
    stages: Stages,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Stats {
    nodes: u64,
    chains: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Stages {
    extractor: Stage,
    auditor: Stage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Stage {
    model: String,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(
    tag = "role",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[allow(dead_code)]
enum Message {
    User {
        id: String,
        content: String,
        timestamp: u64,
    },
    Assistant {
        id: String,
        content: Vec<Text>,
        api: String,
        provider: String,
        model: String,
        usage: Box<Usage>,
        stop_reason: String,
        timestamp: u64,
        response_id: String,
        raw_stop_reason: String,
        duration_ms: u64,
        thinking_level: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Text {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Usage {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total_tokens: u64,
    cost: Cost,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Cost {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    total: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Stamp {
    version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Anchor {
    chat_id: String,
    message_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Node {
    id: String,
    name: String,
    statement: String,
    aliases: Vec<String>,
    created_at: String,
    provenance: Vec<Anchor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Chain {
    id: String,
    premises: Vec<String>,
    conclusion: String,
    created_at: String,
    provenance: Vec<Anchor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Coverage {
    chat_id: String,
    fp: String,
    tokens: u64,
    folded_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Positions {
    positions: BTreeMap<String, Point>,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct Point {
    x: f64,
    y: f64,
}

type Messages = BTreeMap<String, BTreeSet<String>>;
fn require(ok: bool, reason: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    // The native composer embeds quoted filesystem paths inside ordinary text.
    // QA deliberately refuses all path separators, including innocent fractions and URLs.
    fn contains_path(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(text) => text.contains(['/', '\\']),
            serde_json::Value::Array(items) => items.iter().any(contains_path),
            serde_json::Value::Object(fields) => fields
                .iter()
                .any(|(key, v)| key.contains(['/', '\\']) || contains_path(v)),
            _ => false,
        }
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "Malformed record")?;
    require(
        !contains_path(&value),
        "Potential nonportable file reference",
    )?;
    serde_json::from_slice(bytes).map_err(|_| "Malformed or unsupported 0.6.1 record".into())
}
fn lines<T: DeserializeOwned>(bytes: &[u8]) -> Result<Vec<T>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid ledger encoding")?;
    require(text.ends_with('\n'), "Incomplete ledger")?;
    text.lines().map(|line| decode(line.as_bytes())).collect()
}
fn chat_ids(chat: Chat, path: &str) -> Result<BTreeSet<String>, String> {
    require(
        chat.version == 2
            && path == format!("chats/{}.json", chat.id)
            && !chat.title.trim().is_empty()
            && !chat.created_at.is_empty()
            && !chat.updated_at.is_empty(),
        "Unsupported chat version or identity",
    )?;
    if let Some(s) = chat.settings {
        require(
            s.model.provider == "custom" && s.model.id == "qa-model",
            "Only the isolated test model has been exercised",
        )?;
    }
    let mut ids = BTreeSet::new();
    let mut assistants = BTreeSet::new();
    for m in chat.messages {
        let id = match m {
            Message::User { id, .. } => id,
            Message::Assistant {
                id,
                api,
                provider,
                model,
                stop_reason,
                content,
                ..
            } => {
                require(
                    api == "openai-completions"
                        && provider == "custom"
                        && model == "qa-model"
                        && stop_reason == "stop"
                        && !content.is_empty()
                        && content.iter().all(|c| c.kind == "text"),
                    "Unsupported model turn or attachment",
                )?;
                assistants.insert(id.clone());
                id
            }
        };
        require(
            generated_id(&id, "msg_") && ids.insert(id),
            "Invalid or duplicate message identity",
        )?;
    }
    let mut turns = BTreeSet::new();
    for t in chat.turns {
        require(
            assistants.remove(&t.message_id) && turns.insert(t.id) && t.model == "custom:qa-model",
            "Invalid turn association",
        )?;
    }
    require(assistants.is_empty(), "Missing turn metadata")?;
    for e in chat.extractions {
        require(
            [e.stages.auditor, e.stages.extractor]
                .iter()
                .all(|s| s.model == "custom:qa-model"),
            "Unknown extraction model",
        )?;
    }
    Ok(ids)
}
fn anchors(rows: &[Anchor], messages: &Messages, allow_chat: bool) -> bool {
    !rows.is_empty()
        && rows.iter().all(|a| {
            messages.get(&a.chat_id).is_some_and(|ids| {
                ids.contains(&a.message_id) || (allow_chat && a.message_id.is_empty())
            })
        })
}
pub fn validate(payload: &Payload) -> Result<(), String> {
    require(
        !payload.is_empty() && payload.len() <= MAX_FILES,
        "Invalid inventory",
    )?;
    let mut total = 0;
    let mut messages = Messages::new();
    let mut has_extraction = false;
    for (path, bytes) in payload {
        require(
            supported_path(path) && bytes.len() <= MAX_FILE && bytes.len() <= MAX_TOTAL - total,
            "Unknown file or size limit exceeded",
        )?;
        total += bytes.len();
        if path.starts_with("chats/") {
            let chat: Chat = decode(bytes)?;
            has_extraction |= !chat.extractions.is_empty();
            messages.insert(chat.id.clone(), chat_ids(chat, path)?);
        }
    }
    require(!messages.is_empty(), "Missing sessions")?;
    if !payload.keys().any(|k| k.starts_with("graph/")) {
        return require(!has_extraction, "Missing graph for completed extraction");
    }
    let file = |p: &str| {
        payload
            .get(p)
            .map(Vec::as_slice)
            .ok_or_else(|| "Incomplete graph inventory".to_string())
    };
    require(
        decode::<Stamp>(file(GRAPH[0])?)?.version == 1,
        "Unknown graph format",
    )?;
    let mut nodes = BTreeSet::new();
    for n in lines::<Node>(file(GRAPH[1])?)? {
        require(
            n.id.starts_with("n_")
                && nodes.insert(n.id)
                && anchors(&n.provenance, &messages, false),
            "Unsupported node or missing message",
        )?;
    }
    let mut chains = BTreeSet::new();
    let mut junctions = BTreeSet::new();
    for c in lines::<Chain>(file(GRAPH[2])?)? {
        if c.premises.len() > 1 {
            junctions.insert(c.id.clone());
        }
        require(
            c.id.starts_with("arg_")
                && chains.insert(c.id)
                && !c.premises.is_empty()
                && c.premises.iter().all(|p| nodes.contains(p))
                && nodes.contains(&c.conclusion)
                && anchors(&c.provenance, &messages, true),
            "Invalid chain or missing endpoint",
        )?;
    }
    for row in lines::<Coverage>(file(GRAPH[3])?)? {
        require(
            messages.contains_key(&row.chat_id) && super::valid_id(&row.fp),
            "Invalid coverage",
        )?;
    }
    let positions: Positions = decode(file(GRAPH[4])?)?;
    require(
        positions
            .positions
            .keys()
            .all(|p| nodes.contains(p) || junctions.contains(p)),
        "Unknown layout node",
    )
}
