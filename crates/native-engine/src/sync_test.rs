//! A synthetic test succeeds only with a durable, authenticated remote receipt.
use crate::{
    adapter,
    engine::Engine,
    network::Receipt,
    storage::{atomic, nonce, now, read, Store, MAX_METADATA},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    pub time: u64,
    pub receipt: Receipt,
}

pub fn exchanges(s: &Store) -> Result<BTreeMap<String, Exchange>> {
    let path = s.root.join("exchanges.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let items: BTreeMap<String, Exchange> = serde_json::from_slice(&read(&path, MAX_METADATA)?)?;
    ensure!(items.len() <= 64, "Exchange history exceeds limit");
    Ok(items)
}
pub fn latest(s: &Store) -> Result<Value> {
    let path = s.root.join("sync-test.json");
    if !path.exists() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_slice(&read(&path, MAX_METADATA)?)?)
}
async fn save(e: &Engine, result: Value) -> Result<Value> {
    e.disk(move |s| {
        atomic(
            &s.root.join("sync-test.json"),
            &serde_json::to_vec(&result)?,
        )?;
        Ok(result)
    })
    .await
}
pub async fn run(e: &Arc<Engine>, device: &str) -> Result<Value> {
    let _guard = e
        .test_lock
        .try_lock()
        .context("A sync test is already running")?;
    let status = e.status().await?;
    let peer = status["transport"]["peers"]
        .as_array()
        .context("Missing peers")?
        .iter()
        .find(|p| p["id"] == device)
        .context("Choose a paired computer")?;
    ensure!(
        peer["connected"] == true,
        "This computer is offline. Open LearnHarness Sync on both computers and try again."
    );
    ensure!(
        status["conflicts"].as_array().is_some_and(|v| v.is_empty()),
        "Two versions need attention. Review the preserved conflict before testing again."
    );
    let device = device.to_string();
    let name = peer["name"].clone();
    let started = now();
    let token = nonce()?;
    // Initialize and publish under one serialized disk operation. This cannot
    // touch LearnHarness; Engine::start requires a synthetic-only profile marker.
    let author = e.identity.id.clone();
    let snapshot = e
        .disk(move |s| {
            ensure!(
                s.ledger()?.conflicts.is_empty(),
                "Conflict preserved; test not started"
            );
            s.initialize()?;
            s.publish(
                &author,
                &adapter::fixture(&format!(
                    "LearnHarness Sync harmless connection test {token}"
                ))?,
            )
        })
        .await?;
    let mut result = json!({"state":"pending","device":device,"name":name,"snapshot":snapshot,"started":started,"finished":null,"message":"Waiting for the other computer to verify and adopt the test data."});
    save(e, result.clone()).await?;
    let outcome = async {
        for _ in 0..140 {
            let id = device.clone();
            let (ledger, exchange) = e.disk(move |s| Ok((s.ledger()?, exchanges(s)?.remove(&id)))).await?;
            if !ledger.conflicts.is_empty() || exchange.as_ref().is_some_and(|x| x.receipt.conflict) {
                return Ok::<_,anyhow::Error>(("conflict", "Two versions need attention. Both versions have been preserved."));
            }
            if let Some(x) = exchange {
                if x.time >= started && x.receipt.head.as_deref() == Some(&snapshot)
                    && x.receipt.received.as_deref() == Some(&snapshot)
                    && ledger.head.as_deref() == Some(&snapshot) {
                    return Ok(("success", "The other computer received, verified and adopted your test data."));
                }
            }
            let s = e.status().await?;
            let connected = s["transport"]["peers"].as_array().is_some_and(|ps| ps.iter().any(|p| p["id"] == device && p["connected"] == true));
            if !connected { return Ok(("failed", "The connection was interrupted. Open LearnHarness Sync on both computers, then run the test again.")); }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Ok(("pending", "The other computer has not confirmed the test yet. Keep both apps open. You can safely run a new test."))
    }.await;
    let (state, message) = outcome.unwrap_or(("failed", "Verification could not finish. Your history is retained; check Advanced diagnostics before retrying."));
    result["state"] = json!(state);
    result["message"] = json!(message);
    result["finished"] = json!(now());
    save(e, result).await
}
