//! Isolated developer integration harness. Never bundled or launched by the desktop.
use anyhow::{ensure, Context, Result};
use harness_sync_native::Engine;
use serde_json::{json, Value};
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
};
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(all(target_os = "linux", feature = "learnharness-qa"))]
    if args.len() == 4 && args[1] == "--qa-transaction" {
        return harness_sync_native::managed_qa::run(&args[2], &args[3]);
    }
    ensure!(
        args.iter().any(|a| a == "--synthetic-only"),
        "This test harness requires --synthetic-only"
    );
    let get = |name: &str| {
        args.iter()
            .position(|s| s == name)
            .and_then(|i| args.get(i + 1))
            .context("Use --profile NEW-ABSOLUTE-PATH --name LABEL --synthetic-only")
    };
    let profile = PathBuf::from(get("--profile")?);
    let name = get("--name")?.to_string();
    let lan = args.iter().any(|a| a == "--lan");
    let bind = if lan { "0.0.0.0:0" } else { "127.0.0.1:0" };
    let e = Engine::start(profile, name, bind.parse()?, lan).await?;
    println!("{}", json!({"ready":true,"identity":e.identity.id}));
    std::io::stdout().flush()?;
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    loop {
        let mut line = String::new();
        let n = Read::by_ref(&mut reader)
            .take(96 * 1024 + 1)
            .read_line(&mut line)?;
        if n == 0 {
            break;
        }
        ensure!(n <= 96 * 1024, "Test request exceeds limit");
        let request: Value = serde_json::from_str(&line)?;
        let command = request["command"].as_str().context("Command missing")?;
        if command == "exit" {
            break;
        }
        #[cfg(all(target_os = "linux", feature = "learnharness-qa"))]
        if matches!(command, "qa_capture" | "qa_stage_received") {
            let store = e.store.lock().unwrap();
            let result = match command {
                "qa_capture" => harness_sync_native::managed_qa::capture(&store, &e.identity.id)
                    .map(|id| json!(id)),
                _ => harness_sync_native::managed_qa::stage_received(
                    &store,
                    request["args"]["head"].as_str().unwrap_or(""),
                )
                .map(|()| json!(true)),
            };
            println!(
                "{}",
                match result {
                    Ok(v) => json!({"result":v}),
                    Err(e) => json!({"error":e.to_string()}),
                }
            );
            std::io::stdout().flush()?;
            continue;
        }
        #[cfg(feature = "learnharness-qa")]
        if command == "qa_publish" || command == "qa_read" {
            let store = e.store.lock().unwrap();
            let result = (|| -> Result<Value> {
                if command == "qa_publish" {
                    let files: std::collections::BTreeMap<String, String> =
                        serde_json::from_value(request["args"].clone())?;
                    let payload = files
                        .into_iter()
                        .map(|(k, v)| (k, v.into_bytes()))
                        .collect();
                    harness_sync_core::learnharness_061::validate(&payload)
                        .map_err(anyhow::Error::msg)?;
                    Ok(json!(store.publish(&e.identity.id, &payload)?))
                } else {
                    let manifest = store
                        .manifest(request["args"]["head"].as_str().context("Expected head")?)?;
                    let files = store.payload(&manifest)?;
                    harness_sync_core::learnharness_061::validate(&files)
                        .map_err(anyhow::Error::msg)?;
                    let files = files
                        .into_iter()
                        .map(|(k, v)| Ok((k, String::from_utf8(v)?)))
                        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
                    Ok(json!(files))
                }
            })();
            println!(
                "{}",
                match result {
                    Ok(v) => json!({"result":v}),
                    Err(e) => json!({"error":e.to_string()}),
                }
            );
            std::io::stdout().flush()?;
            continue;
        }
        let response = match e
            .request(
                command,
                request.get("args").cloned().unwrap_or_else(|| json!({})),
            )
            .await
        {
            Ok(v) => json!({"result":v}),
            Err(e) => json!({"error":e.to_string()}),
        };
        println!("{response}");
        std::io::stdout().flush()?;
    }
    e.shutdown();
    Ok(())
}
