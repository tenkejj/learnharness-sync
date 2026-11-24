//! Versioned, deterministic synthetic snapshot model. No filesystem or transport authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub mod learnharness_061;

pub const FORMAT: &str = "harness-sync-synthetic-v2";
pub const MAX_FILE: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL: usize = 128 * 1024 * 1024;
pub const MAX_FILES: usize = 4096;
pub const MAX_HISTORY: usize = 4096;
pub type Payload = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub snapshot_id: String,
    pub parent_ids: Vec<String>,
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub hash: String,
    pub size: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub format: String,
    pub device: String,
    pub parents: Vec<String>,
    pub files: Vec<FileEntry>,
}
impl Manifest {
    pub fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serializable manifest")
    }
    pub fn id(&self) -> String {
        sha256(&self.bytes())
    }
    pub fn validate(&self) -> Result<(), String> {
        let supported = self.format == FORMAT;
        #[cfg(feature = "learnharness-qa")]
        let supported = supported || self.format == learnharness_061::FORMAT;
        if self.version != 2
            || !supported
            || !valid_id(&self.device)
            || self.parents.len() > 1
            || self.parents.iter().any(|p| !valid_id(p))
            || self.files.len() > MAX_FILES
        {
            return Err("Incompatible snapshot manifest".into());
        }
        let mut total = 0u64;
        let mut previous: Option<&str> = None;
        for file in &self.files {
            #[cfg(feature = "learnharness-qa")]
            let qa_path = self.format == learnharness_061::FORMAT;
            #[cfg(not(feature = "learnharness-qa"))]
            let qa_path = false;
            if !qa_path {
                valid_path(&file.path)?;
            }
            #[cfg(feature = "learnharness-qa")]
            if qa_path && !learnharness_061::supported_path(&file.path) {
                return Err("Unsupported LearnHarness path".into());
            }
            if !valid_id(&file.hash)
                || file.size > MAX_FILE as u64
                || previous.is_some_and(|p| p >= file.path.as_str())
            {
                return Err("Invalid or unordered inventory".into());
            }
            total = total.checked_add(file.size).ok_or("Inventory overflow")?;
            previous = Some(&file.path);
        }
        if total > MAX_TOTAL as u64 {
            return Err("Snapshot exceeds size limit".into());
        }
        Ok(())
    }
    pub fn from_payload(
        device: String,
        parents: Vec<String>,
        payload: &Payload,
    ) -> Result<Self, String> {
        let format = FORMAT;
        #[cfg(feature = "learnharness-qa")]
        let format = if learnharness_061::validate(payload).is_ok() {
            learnharness_061::FORMAT
        } else {
            format
        };
        validate_format(format, payload)?;
        let result = Self {
            version: 2,
            format: format.into(),
            device,
            parents,
            files: payload
                .iter()
                .map(|(path, bytes)| FileEntry {
                    path: path.clone(),
                    hash: sha256(bytes),
                    size: bytes.len() as u64,
                })
                .collect(),
        };
        result.validate()?;
        Ok(result)
    }
}
// Deliberately narrow fixture adapter, never presented as LearnHarness compatibility.
pub fn valid_path(path: &str) -> Result<(), String> {
    let parts: Vec<_> = path.split('/').collect();
    let allowed = path == "settings.json"
        || (parts.len() == 2
            && ["chats", "graph", "jobs"].contains(&parts[0])
            && parts[1].strip_suffix(".json").is_some_and(|stem| {
                !stem.is_empty()
                    && stem
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            }));
    if !allowed
        || ["auth", "token", "credential", "secret", "debug", "trace"]
            .iter()
            .any(|s| path.to_lowercase().contains(s))
    {
        return Err("Unsupported or unsafe payload path".into());
    }
    Ok(())
}
pub fn validate_format(format: &str, payload: &Payload) -> Result<(), String> {
    match format {
        FORMAT => validate_payload(payload),
        #[cfg(feature = "learnharness-qa")]
        learnharness_061::FORMAT => learnharness_061::validate(payload),
        _ => Err("Unsupported payload format".into()),
    }
}
pub fn validate_payload(payload: &Payload) -> Result<(), String> {
    if payload.len() > MAX_FILES || payload.values().map(Vec::len).sum::<usize>() > MAX_TOTAL {
        return Err("Payload exceeds limits".into());
    }
    for (path, bytes) in payload {
        valid_path(path)?;
        if bytes.len() > MAX_FILE {
            return Err("File exceeds size limit".into());
        }
        let obj: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| "Malformed fixture JSON")?;
        if !obj.is_object() || obj.get("version").and_then(|v| v.as_u64()) != Some(1) {
            return Err("Unknown fixture schema".into());
        }
        let allowed: &[&str] = if path == "settings.json" || path == "graph/format.json" {
            &["version"]
        } else if path.starts_with("chats/") {
            &["version", "id", "title", "text"]
        } else {
            return Err("Unknown synthetic record schema".into());
        };
        let fields = obj.as_object().ok_or("Expected synthetic object")?;
        if fields.len() != allowed.len() || allowed.iter().any(|k| !fields.contains_key(*k)) {
            return Err("Unknown or missing synthetic fields".into());
        }
        if path.starts_with("chats/")
            && ["id", "title", "text"].iter().any(|k| !obj[*k].is_string())
        {
            return Err("Malformed synthetic chat".into());
        }
        if path.starts_with("chats/")
            && obj.get("id").and_then(|v| v.as_str())
                != path
                    .strip_prefix("chats/")
                    .and_then(|s| s.strip_suffix(".json"))
        {
            return Err("Chat identity mismatch".into());
        }
        reject_secrets(&obj)?;
    }
    Ok(())
}
fn reject_secrets(v: &serde_json::Value) -> Result<(), String> {
    match v {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if [
                    "apikey",
                    "api_key",
                    "authorization",
                    "access_token",
                    "refresh_token",
                    "password",
                    "secret",
                ]
                .contains(&k.to_lowercase().as_str())
                {
                    return Err("Credential-like fixture field refused".into());
                }
                reject_secrets(v)?;
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                reject_secrets(v)?;
            }
        }
        _ => (),
    }
    Ok(())
}
/// Validate the COMPLETE reachable graph before deciding ancestry (including on equal heads).
pub fn is_ancestor(
    old: &str,
    new: &str,
    history: &BTreeMap<String, Revision>,
) -> Result<bool, String> {
    let mut seen = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut pending = vec![(new.to_string(), false)];
    while let Some((id, finished)) = pending.pop() {
        if finished {
            active.remove(&id);
            seen.insert(id);
            continue;
        }
        if active.contains(&id) {
            return Err("Cyclic ancestry".into());
        }
        if seen.contains(&id) {
            continue;
        }
        if seen.len() + active.len() >= MAX_HISTORY {
            return Err("History limit exceeded".into());
        }
        let revision = history.get(&id).ok_or("Unknown ancestry")?;
        if revision.snapshot_id != id {
            return Err("History identity mismatch".into());
        }
        active.insert(id.clone());
        pending.push((id, true));
        for parent in revision.parent_ids.iter().rev() {
            pending.push((parent.clone(), false));
        }
    }
    Ok(seen.contains(old))
}
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Equal,
    FastForward,
    KeepLocal,
    Conflict,
}
pub fn reconcile(
    local: Option<&str>,
    remote: &str,
    h: &BTreeMap<String, Revision>,
) -> Result<Decision, String> {
    is_ancestor(remote, remote, h)?;
    let Some(local) = local else {
        return Ok(Decision::FastForward);
    };
    is_ancestor(local, local, h)?;
    if local == remote {
        Ok(Decision::Equal)
    } else if is_ancestor(local, remote, h)? {
        Ok(Decision::FastForward)
    } else if is_ancestor(remote, local, h)? {
        Ok(Decision::KeepLocal)
    } else {
        Ok(Decision::Conflict)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes_and_unknown_ancestry() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_ancestor("a", "b", &BTreeMap::new()).is_err());
    }
    fn history() -> BTreeMap<String, Revision> {
        [("a", vec![]), ("b", vec!["a"]), ("c", vec!["a"])]
            .into_iter()
            .map(|(s, p)| {
                (
                    s.into(),
                    Revision {
                        snapshot_id: s.into(),
                        parent_ids: p.into_iter().map(str::to_owned).collect(),
                    },
                )
            })
            .collect()
    }
    #[test]
    fn causal_order() {
        let h = history();
        assert_eq!(is_ancestor("a", "b", &h), Ok(true));
        assert_eq!(reconcile(Some("b"), "c", &h), Ok(Decision::Conflict));
        assert_eq!(reconcile(Some("a"), "b", &h), Ok(Decision::FastForward));
        assert_eq!(reconcile(Some("b"), "a", &h), Ok(Decision::KeepLocal));
    }
    #[test]
    fn equal_missing_history_fails() {
        let mut h = history();
        h.remove("a");
        assert!(reconcile(Some("b"), "b", &h).is_err());
    }
    #[test]
    fn cycle_fails() {
        let mut h = history();
        h.get_mut("a").unwrap().parent_ids.push("b".into());
        assert!(is_ancestor("a", "b", &h).is_err());
    }
    #[test]
    fn paths_fail_closed() {
        for p in [
            "../settings.json",
            "/settings.json",
            "chats/../x.json",
            "chats/auth.json",
            "chats/a\\b.json",
            "chats/a:b.json",
            "chats/.json",
        ] {
            assert!(valid_path(p).is_err(), "{p}");
        }
    }
    #[test]
    fn schema_fails_closed() {
        let p = BTreeMap::from([("settings.json".into(), b"{\"version\":2}".to_vec())]);
        assert!(validate_payload(&p).is_err());
    }
    #[test]
    fn manifest_deterministic() {
        let p = BTreeMap::from([("settings.json".into(), b"{\"version\":1}".to_vec())]);
        let m = Manifest::from_payload("a".repeat(64), vec![], &p).unwrap();
        assert_eq!(
            m.id(),
            "3d68e9c35be8751759bc58d7ab0dd01679349856a21040bf8ac02cb6592fa51d"
        );
        assert_eq!(m.bytes(), serde_json::to_vec(&m).unwrap());
    }
}
