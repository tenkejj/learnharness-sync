#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use anyhow::Context;
use anyhow::Result;
use harness_sync_core::{sha256, Decision, Manifest};
use harness_sync_native::{
    adapter,
    engine::{Engine, Peer},
    learnharness_export::ExportManifest,
    network,
    storage::{self, Store},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;

async fn engine(temp: &TempDir, name: &str) -> Arc<Engine> {
    Engine::start(
        temp.path().join(name),
        name.into(),
        "127.0.0.1:0".parse().unwrap(),
        false,
    )
    .await
    .unwrap()
}
fn peer(e: &Engine) -> Peer {
    Peer {
        id: e.identity.id.clone(),
        name: e.name(),
        certificate: e.identity.certificate.clone(),
        address: e.local_addr().unwrap().to_string(),
    }
}
async fn wait_for(e: &Engine, predicate: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    for _ in 0..800 {
        let s = e.status().await.unwrap();
        if predicate(&s) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "Timed out waiting for native state: {}",
        e.status().await.unwrap()
    );
}
async fn pair(a: &Arc<Engine>, b: &Arc<Engine>, approve: bool) -> Result<()> {
    a.request("open_pairing", json!({})).await?;
    b.request("open_pairing", json!({})).await?;
    let aa = a.clone();
    let p = peer(b);
    let task = tokio::spawn(async move { network::connect(aa, p, true).await });
    let sa = wait_for(a, |s| !s["pending"].as_array().unwrap().is_empty()).await;
    let sb = wait_for(b, |s| !s["pending"].as_array().unwrap().is_empty()).await;
    assert_eq!(sa["pending"][0]["code"], sb["pending"][0]["code"]);
    assert_eq!(sa["transport"]["peers"], json!([]));
    a.request(
        "pairing_decision",
        json!({"id":sa["pending"][0]["id"],"accept":approve}),
    )
    .await?;
    b.request(
        "pairing_decision",
        json!({"id":sb["pending"][0]["id"],"accept":approve}),
    )
    .await?;
    task.await?
}
async fn init(e: &Arc<Engine>) {
    e.request("initialize_synthetic", json!({"confirmed":true}))
        .await
        .unwrap();
}
async fn edit(e: &Arc<Engine>, text: &str) -> String {
    e.request("synthetic_edit", json!({"text":text}))
        .await
        .unwrap()["snapshot"]
        .as_str()
        .unwrap()
        .into()
}

fn valid_learnharness_payload() -> std::collections::BTreeMap<String, Vec<u8>> {
    let chat = serde_json::json!({
        "version":2,"id":"chat_abc123_1234abcd","title":"Disposable export test",
        "createdAt":"2026-11-24T10:00:00.000Z","updatedAt":"2026-11-24T10:00:00.000Z",
        "messages":[{"role":"user","id":"msg_abc123_1234abcd","content":"Disposable unicode ✓ conversation","timestamp":1795514400000i64}]
    });
    let node = serde_json::json!({
        "id":"n_export","name":"Export node","statement":"A relationship survives transfer.","aliases":[],
        "createdAt":"2026-11-24T10:00:00.000Z",
        "provenance":[{"chatId":"chat_abc123_1234abcd","messageId":"msg_abc123_1234abcd"}]
    });
    let chain = serde_json::json!({
        "id":"arg_export","premises":["n_export"],"conclusion":"n_export",
        "createdAt":"2026-11-24T10:00:00.000Z",
        "provenance":[{"chatId":"chat_abc123_1234abcd","messageId":""}]
    });
    let rows = [
        ("chats/chat_abc123_1234abcd.json", serde_json::to_vec(&chat).unwrap()),
        ("graph/format.json", br#"{"version":1}"#.to_vec()),
        ("graph/nodes.jsonl", [serde_json::to_vec(&node).unwrap(), b"\n".to_vec()].concat()),
        ("graph/chains.jsonl", [serde_json::to_vec(&chain).unwrap(), b"\n".to_vec()].concat()),
        ("graph/coverage.jsonl", [serde_json::to_vec(&serde_json::json!({"chatId":"chat_abc123_1234abcd","fp":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","tokens":1,"foldedAt":"2026-11-24T10:00:00.000Z"})).unwrap(), b"\n".to_vec()].concat()),
        ("graph/positions.json", br#"{"positions":{"n_export":{"x":1.0,"y":2.0}},"updatedAt":"2026-11-24T10:00:00.000Z"}"#.to_vec()),
    ];
    rows.into_iter().map(|(p, b)| (p.to_owned(), b)).collect()
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn managed_merge_preserves_both_chat_branches_and_rejects_identity_edits() -> Result<()> {
    let base = valid_learnharness_payload();
    let mut local = base.clone();
    let mut incoming = base.clone();
    let local_only = serde_json::json!({
        "version":2,"id":"chat_local123_1234abcd","title":"Local-only session",
        "createdAt":"2026-11-24T10:00:00.000Z","updatedAt":"2026-11-24T10:00:00.000Z",
        "messages":[{"role":"user","id":"msg_localonly_1234abcd","content":"Keep this independent conversation","timestamp":1795514400000i64}]
    });
    local.insert(
        "chats/chat_local123_1234abcd.json".into(),
        serde_json::to_vec(&local_only)?,
    );
    for (payload, id, body, timestamp) in [
        (
            &mut local,
            "msg_local_1234abcd",
            "local branch ✓",
            1795514401000i64,
        ),
        (
            &mut incoming,
            "msg_remote_1234abcd",
            "remote branch 🌍",
            1795514402000i64,
        ),
    ] {
        let mut chat: serde_json::Value =
            serde_json::from_slice(&payload["chats/chat_abc123_1234abcd.json"])?;
        chat["messages"].as_array_mut().unwrap().push(json!({
            "role":"user", "id":id, "content":body, "timestamp":timestamp
        }));
        chat["updatedAt"] = json!("2026-11-24T10:00:02.000Z");
        payload.insert(
            "chats/chat_abc123_1234abcd.json".into(),
            serde_json::to_vec(&chat)?,
        );
    }

    let merged = harness_sync_native::managed_profile::merge_records(&local, &incoming)?;
    harness_sync_native::learnharness_export::validate_payload(&merged)?;
    let chat: serde_json::Value =
        serde_json::from_slice(&merged["chats/chat_abc123_1234abcd.json"])?;
    let ids: Vec<_> = chat["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"msg_local_1234abcd"));
    assert!(ids.contains(&"msg_remote_1234abcd"));
    assert_eq!(
        merged["chats/chat_local123_1234abcd.json"],
        serde_json::to_vec(&local_only)?
    );

    let mut conflicting = incoming;
    let mut chat: serde_json::Value =
        serde_json::from_slice(&conflicting["chats/chat_abc123_1234abcd.json"])?;
    chat["messages"][0]["content"] = json!("same identity, different bytes");
    conflicting.insert(
        "chats/chat_abc123_1234abcd.json".into(),
        serde_json::to_vec(&chat)?,
    );
    assert!(harness_sync_native::managed_profile::merge_records(&local, &conflicting).is_err());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "Requires running inside the managed mapped-UID vault so Btrfs cleanup is permitted"]
fn btrfs_managed_apply_creates_independent_backup_and_recovers_unverified_generation() -> Result<()>
{
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    let temp = tempfile::Builder::new()
        .prefix("learnharness-apply-test-")
        .tempdir_in(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target"))?;
    let vault = temp.path().join("vault");
    fs::create_dir(&vault)?;
    fs::set_permissions(&vault, fs::Permissions::from_mode(0o700))?;
    let generation = format!(
        "session.{}",
        std::fs::read_to_string("/proc/sys/kernel/random/uuid")?.trim()
    );
    let old = vault.join(&generation);
    let create = Command::new("btrfs")
        .args(["subvolume", "create"])
        .arg(&old)
        .output()?;
    if !create.status.success() {
        eprintln!(
            "Btrfs apply test unavailable: {}",
            String::from_utf8_lossy(&create.stderr)
        );
        return Ok(());
    }
    let backups = vault.join("backups");
    fs::create_dir(&backups)?;
    fs::set_permissions(&backups, fs::Permissions::from_mode(0o700))?;
    let initial = valid_learnharness_payload();
    let initial_tree =
        harness_sync_native::managed_profile::Tree::from_records(initial.clone(), None)?;
    let data = old.join("config/LearnHarness/data");
    fs::create_dir_all(data.parent().unwrap())?;
    initial_tree.copy_to(&data)?;
    Command::new("btrfs")
        .args(["property", "set", "-ts"])
        .arg(&old)
        .args(["ro", "true"])
        .output()?
        .status
        .success()
        .then_some(())
        .context("Could not make test source generation immutable")?;
    fs::write(vault.join("current"), format!("{generation}\n"))?;

    let mut incoming = initial.clone();
    let mut chat: serde_json::Value =
        serde_json::from_slice(&incoming["chats/chat_abc123_1234abcd.json"])?;
    chat["messages"].as_array_mut().unwrap().push(json!({
        "role":"user", "id":"msg_incoming_1234abcd", "content":"incoming disposable branch", "timestamp":1795514403000i64
    }));
    chat["updatedAt"] = json!("2026-11-24T10:00:03.000Z");
    incoming.insert(
        "chats/chat_abc123_1234abcd.json".into(),
        serde_json::to_vec(&chat)?,
    );
    let manifest = ExportManifest::from_payload(&incoming)?;
    let revision = manifest.revision.clone();
    fs::write(vault.join("active"), b"apply\n")?;
    let receipt =
        harness_sync_native::managed_profile::apply_incoming(&vault, &manifest, &incoming)?;
    assert_eq!(receipt["state"], "awaiting_readback");
    assert_eq!(fs::read(vault.join("active"))?, b"apply\n");

    let applied_generation = fs::read_to_string(vault.join("current"))?.trim().to_owned();
    assert_ne!(applied_generation, generation);
    let applied = harness_sync_native::managed_profile::Tree::records(
        &vault
            .join(&applied_generation)
            .join("config/LearnHarness/data"),
    )?;
    assert!(
        String::from_utf8(applied["chats/chat_abc123_1234abcd.json"].clone())?
            .contains("msg_incoming_1234abcd")
    );
    let backup = vault
        .join("backups")
        .join(receipt["backup"].as_str().unwrap());
    assert_eq!(
        fs::read(backup.join("config/LearnHarness/data/chats/chat_abc123_1234abcd.json"))?,
        initial["chats/chat_abc123_1234abcd.json"]
    );

    let confirmation = harness_sync_native::managed_profile::confirm_readback(&vault, &revision)?;
    assert_eq!(confirmation["confirmed"], true);
    fs::write(vault.join("active"), b"apply\n")?;
    let mut divergent = applied.clone();
    let mut chat: serde_json::Value =
        serde_json::from_slice(&divergent["chats/chat_abc123_1234abcd.json"])?;
    chat["messages"].as_array_mut().unwrap().push(json!({
        "role":"user", "id":"msg_second_1234abcd", "content":"unverified disposable branch", "timestamp":1795514404000i64
    }));
    chat["updatedAt"] = json!("2026-11-24T10:00:04.000Z");
    divergent.insert(
        "chats/chat_abc123_1234abcd.json".into(),
        serde_json::to_vec(&chat)?,
    );
    let second_manifest = ExportManifest::from_payload(&divergent)?;
    let second_revision = second_manifest.revision.clone();
    fs::write(vault.join("active"), b"apply\n")?;
    harness_sync_native::managed_profile::apply_incoming(&vault, &second_manifest, &divergent)?;
    let second_generation = fs::read_to_string(vault.join("current"))?.trim().to_owned();
    assert_ne!(second_generation, applied_generation);
    let recovered = harness_sync_native::managed_profile::recover_transaction(&vault)?;
    assert_eq!(recovered["outcome"], "rolled_back_unverified_apply");
    assert_eq!(
        fs::read_to_string(vault.join("current"))?.trim(),
        applied_generation
    );
    assert!(harness_sync_core::valid_id(&second_revision));

    // Remove only the exact disposable subvolumes created by this test.
    for path in [
        old.clone(),
        vault.join(&applied_generation),
        vault.join(&second_generation),
        backup,
    ] {
        if path.exists() {
            let _ = Command::new("btrfs")
                .args(["subvolume", "delete"])
                .arg(&path)
                .status()?;
        }
    }
    for entry in fs::read_dir(&backups)? {
        let path = entry?.path();
        let _ = Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(path)
            .status()?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
async fn managed_export_uses_authenticated_quic_and_stages_without_reconciliation() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "ManagedA").await;
    let b = engine(&t, "ManagedB").await;
    a.trust(peer(&b))?;
    b.trust(peer(&a))?;
    let original_b_status = b.status().await?;
    let payload = valid_learnharness_payload();
    harness_sync_native::learnharness_export::validate_payload(&payload)?;
    let receiver_profile = t
        .path()
        .join("ManagedB-app-profile/config/LearnHarness/data");
    storage::private_dir(receiver_profile.parent().unwrap())?;
    harness_sync_native::managed_profile::Tree::from_records(payload.clone(), None)?
        .copy_to(&receiver_profile)?;
    let receiver_profile_before =
        harness_sync_native::managed_profile::Tree::read(&receiver_profile)?.id()?;
    let manifest = ExportManifest::from_payload(&payload)?;
    let revision = manifest.revision.clone();
    let receipt = a
        .export_managed(&b.identity.id, manifest, payload.clone(), true)
        .await?;
    assert_eq!(receipt["revision"], revision);
    assert_eq!(receipt["applied"], false);
    let staged_root = t
        .path()
        .join("ManagedB/quarantine/learnharness-0.6.1")
        .join(&revision);
    let mut received = std::collections::BTreeMap::new();
    for path in payload.keys() {
        received.insert(
            path.clone(),
            storage::read(&staged_root.join(path), harness_sync_core::MAX_FILE)?,
        );
    }
    assert_eq!(received, payload);
    let after_b_status = b.status().await?;
    assert_eq!(after_b_status["head"], original_b_status["head"]);
    assert_eq!(after_b_status["snapshots"], original_b_status["snapshots"]);
    assert!(after_b_status["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(
        harness_sync_native::managed_profile::Tree::read(&receiver_profile)?.id()?,
        receiver_profile_before
    );
    // Exact duplicate deliveries are idempotent and still do not reconcile.
    let duplicate = a
        .export_managed(
            &b.identity.id,
            ExportManifest::from_payload(&payload)?,
            payload.clone(),
            true,
        )
        .await?;
    assert_eq!(duplicate["revision"], revision);
    assert_eq!(b.status().await?["head"], original_b_status["head"]);
    a.shutdown();
    b.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_export_requires_sender_consent_and_receiver_trust() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "ConsentA").await;
    let b = engine(&t, "ConsentB").await;
    let payload = valid_learnharness_payload();
    let manifest = ExportManifest::from_payload(&payload)?;
    assert!(a
        .export_managed(&b.identity.id, manifest.clone(), payload.clone(), false)
        .await
        .is_err());
    a.trust(peer(&b))?;
    // The receiving engine has not trusted A, so TLS alone cannot authorize the export.
    assert!(a
        .export_managed(&b.identity.id, manifest, payload, true)
        .await
        .is_err());
    assert!(!t.path().join("ConsentB/quarantine").exists());
    a.shutdown();
    b.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_quic_pair_transfer_bidirectional_offline_conflict_reconnect_revoke() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "Desktop").await;
    let b = engine(&t, "Laptop").await;
    init(&a).await;
    init(&b).await;
    let first = edit(&a, "Invented note one").await;
    pair(&a, &b, true).await?;
    let bb = wait_for(&b, |s| s["head"] == first).await;
    assert_eq!(bb["snapshots"].as_array().unwrap().len(), 1);
    let second = edit(&b, "Invented note two").await;
    wait_for(&a, |s| s["head"] == second).await;
    assert!(a.status().await?["backups"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == first));
    a.request("revoke", json!({"device":b.identity.id})).await?;
    b.request("revoke", json!({"device":a.identity.id})).await?;
    let left = edit(&a, "Offline desktop branch").await;
    let right = edit(&b, "Offline laptop branch").await;
    // Existing history does not grant a new trust relationship.
    assert!(network::connect(a.clone(), peer(&b), false).await.is_err());
    pair(&a, &b, true).await?;
    wait_for(&a, |s| {
        s["conflicts"].as_array().unwrap().contains(&json!(right))
    })
    .await;
    wait_for(&b, |s| {
        s["conflicts"].as_array().unwrap().contains(&json!(left))
    })
    .await;
    assert_eq!(a.status().await?["head"], left);
    assert_eq!(b.status().await?["head"], right);
    let right2 = right.clone();
    a.disk(move |s| {
        s.history(&right2)?;
        s.history(&left)?;
        Ok(())
    })
    .await?;
    a.shutdown();
    b.shutdown();
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejecting_pairing_persists_no_trust_and_untrusted_sync_is_refused() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "A").await;
    let b = engine(&t, "B").await;
    assert!(network::connect(a.clone(), peer(&b), false).await.is_err());
    assert!(pair(&a, &b, false).await.is_err());
    tokio::time::sleep(Duration::from_millis(100)).await;
    for e in [&a, &b] {
        assert!(e.status().await?["transport"]["peers"]
            .as_array()
            .unwrap()
            .is_empty());
        let s: serde_json::Value = serde_json::from_slice(&storage::read(
            &t.path().join(e.name()).join("devices.json"),
            16384,
        )?)?;
        assert_eq!(s["peers"], json!({}));
        e.shutdown();
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_certificate_and_wrong_identity_are_rejected() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "A").await;
    let b = engine(&t, "B").await;
    let c = engine(&t, "C").await;
    let mut wrong = peer(&c);
    wrong.address = b.local_addr()?.to_string();
    assert!(network::connect(a.clone(), wrong, true).await.is_err());
    let mut wrong = peer(&b);
    wrong.id = "a".repeat(64);
    assert!(network::connect(a.clone(), wrong, true).await.is_err());
    a.shutdown();
    b.shutdown();
    c.shutdown();
    Ok(())
}
#[test]
fn durable_storage_recovery_duplicates_hashes_ancestry_backups() -> Result<()> {
    let t = TempDir::new()?;
    let root = t.path().join("store");
    let s = Store::open(root.clone())?;
    s.initialize()?;
    let first = s.publish(&"a".repeat(64), &adapter::fixture("first")?)?;
    assert_eq!(
        s.publish(&"a".repeat(64), &adapter::fixture("first")?)?,
        first
    );
    let second = s.publish(&"a".repeat(64), &adapter::fixture("second")?)?;
    assert_eq!(s.reconcile(&first)?, Decision::KeepLocal);
    assert_eq!(s.reconcile(&second)?, Decision::Equal);
    storage::atomic(&root.join("staging").join("interrupted"), b"incomplete")?;
    drop(s);
    let s = Store::open(root.clone())?;
    assert!(!root.join("staging/interrupted").exists());
    assert_eq!(s.ledger()?.head, Some(second.clone()));
    s.history(&second)?;
    assert!(s.put_object(&sha256(b"expected"), b"corrupt").is_err());
    for id in [
        "../ledger.json",
        "x",
        "Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        assert!(s.object(id).is_err());
    }
    let absent = "f".repeat(64);
    let m =
        Manifest::from_payload("b".repeat(64), vec![absent], &adapter::fixture("orphan")?).unwrap();
    for b in adapter::fixture("orphan")?.values() {
        s.put_object(&sha256(b), b)?;
    }
    assert!(s.commit_manifest(&m).is_err());
    assert_eq!(s.ledger()?.head, Some(second));
    Ok(())
}
#[test]
fn fast_forward_backup_and_restore_are_recoverable() -> Result<()> {
    let t = TempDir::new()?;
    let a = Store::open(t.path().join("a"))?;
    let b = Store::open(t.path().join("b"))?;
    a.initialize()?;
    b.initialize()?;
    let first = a.publish(&"a".repeat(64), &adapter::fixture("first")?)?;
    copy(&a, &b, &first)?;
    assert_eq!(b.reconcile(&first)?, Decision::FastForward);
    let second = a.publish(&"a".repeat(64), &adapter::fixture("second")?)?;
    copy(&a, &b, &second)?;
    assert_eq!(b.reconcile(&second)?, Decision::FastForward);
    assert!(b.ledger()?.backups.contains(&first));
    let backup = t.path().join("b/backups").join(&first);
    assert!(backup.join("manifest.json").is_file());
    // A failed restore cannot change current head or remove its backup.
    storage::atomic(&t.path().join("b/WRITER_ACTIVE"), b"quarantine")?;
    assert!(b.restore(&"b".repeat(64), &first).is_err());
    assert_eq!(b.ledger()?.head, Some(second.clone()));
    assert!(backup.exists());
    std::fs::remove_file(t.path().join("b/WRITER_ACTIVE"))?;
    let original = b.manifest(&first)?;
    let chat = original
        .files
        .iter()
        .find(|entry| entry.path.starts_with("chats/"))
        .unwrap();
    std::fs::remove_file(b.root.join("objects").join(&chat.hash))?;
    std::fs::remove_file(b.root.join("manifests").join(&first))?;
    let restored = b.restore(&"b".repeat(64), &first)?;
    assert_ne!(restored, first);
    assert_eq!(b.manifest(&restored)?.parents, vec![second]);
    Ok(())
}
fn copy(a: &Store, b: &Store, head: &str) -> Result<()> {
    for id in a.history(head)?.keys() {
        let m = a.manifest(id)?;
        for f in &m.files {
            b.put_object(&f.hash, &a.object(&f.hash)?)?;
        }
    }
    fn one(a: &Store, b: &Store, id: &str) -> Result<()> {
        let m = a.manifest(id)?;
        for p in &m.parents {
            one(a, b, p)?;
        }
        b.commit_manifest(&m)?;
        Ok(())
    }
    one(a, b, head)
}
#[test]
fn identity_persists_and_corruption_does_not_regenerate() -> Result<()> {
    let t = TempDir::new()?;
    let s = Store::open(t.path().join("s"))?;
    let a = harness_sync_native::identity::Identity::load(&s.root)?;
    let b = harness_sync_native::identity::Identity::load(&s.root)?;
    assert_eq!(a.id, b.id);
    assert!(
        harness_sync_native::identity::verify(&a.certificate, b"other", &a.sign(b"message"))
            .is_err()
    );
    storage::atomic(&s.root.join("identity.json"), b"bad")?;
    assert!(harness_sync_native::identity::Identity::load(&s.root).is_err());
    Ok(())
}
#[test]
fn active_writer_unknown_schema_disk_failure_and_profile_lock_fail_closed() -> Result<()> {
    let t = TempDir::new()?;
    let root = t.path().join("s");
    let s = Store::open(root.clone())?;
    s.initialize()?;
    assert!(Store::open(root.clone()).is_err());
    let bad =
        std::collections::BTreeMap::from([("settings.json".into(), br#"{"version":9}"#.to_vec())]);
    assert!(s.publish(&"a".repeat(64), &bad).is_err());
    storage::atomic(&root.join("WRITER_ACTIVE"), b"writer")?;
    assert!(s
        .publish(&"a".repeat(64), &adapter::fixture("test")?)
        .is_err());
    std::fs::remove_file(root.join("WRITER_ACTIVE"))?;
    std::fs::rename(root.join("staging"), root.join("staging-disabled"))?;
    std::fs::write(root.join("staging"), b"not a directory")?;
    assert!(s
        .publish(&"a".repeat(64), &adapter::fixture("write fails")?)
        .is_err());
    assert!(s.ledger()?.head.is_none());
    assert!(adapter::production_operation().is_err());
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlink_and_hardlink_attacks_fail_closed() -> Result<()> {
    use std::os::unix::fs::symlink;
    let t = TempDir::new()?;
    let s = Store::open(t.path().join("s"))?;
    let victim = t.path().join("victim");
    std::fs::write(&victim, b"untouched")?;
    let id = sha256(b"untouched");
    symlink(&victim, s.root.join("objects").join(&id))?;
    assert!(s.object(&id).is_err());
    assert!(s.put_object(&id, b"untouched").is_err());
    assert_eq!(std::fs::read(&victim)?, b"untouched");
    let hard = s.root.join("hard");
    std::fs::hard_link(&victim, &hard)?;
    assert!(storage::read(&hard, 1024).is_err());
    let linked = t.path().join("linked");
    symlink(&s.root, &linked)?;
    assert!(Store::open(linked).is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonempty_ordinary_directory_cannot_be_adopted_as_test_profile() -> Result<()> {
    let t = TempDir::new()?;
    let root = t.path().join("ordinary");
    storage::private_dir(&root)?;
    std::fs::write(root.join("invented-user-file"), b"must remain untouched")?;
    assert!(
        Engine::start(root.clone(), "Test".into(), "127.0.0.1:0".parse()?, false)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.join("invented-user-file"))?,
        b"must remain untouched"
    );
    assert!(!root.join("identity.json").exists());
    Ok(())
}
#[test]
fn quota_refusal_preserves_head_and_history() -> Result<()> {
    let t = TempDir::new()?;
    let s = Store::open(t.path().join("s"))?;
    s.initialize()?;
    let first = s.publish(&"a".repeat(64), &adapter::fixture("first")?)?;
    // Sparse synthetic file simulates exhausted quota without allocating 1 GiB on disk.
    let file = std::fs::File::create(s.root.join("objects").join("f".repeat(64)))?;
    file.set_len(1024 * 1024 * 1024)?;
    assert!(s
        .publish(&"a".repeat(64), &adapter::fixture("second")?)
        .is_err());
    assert_eq!(s.ledger()?.head, Some(first.clone()));
    s.history(&first)?;
    Ok(())
}
#[cfg(unix)]
#[test]
fn failed_ledger_commit_preserves_original_and_independent_backup() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let t = TempDir::new()?;
    let a = Store::open(t.path().join("a"))?;
    let b = Store::open(t.path().join("b"))?;
    a.initialize()?;
    b.initialize()?;
    let first = a.publish(&"a".repeat(64), &adapter::fixture("first")?)?;
    copy(&a, &b, &first)?;
    b.reconcile(&first)?;
    let second = a.publish(&"a".repeat(64), &adapter::fixture("second")?)?;
    copy(&a, &b, &second)?;
    std::fs::set_permissions(&b.root, std::fs::Permissions::from_mode(0o500))?;
    let result = b.reconcile(&second);
    std::fs::set_permissions(&b.root, std::fs::Permissions::from_mode(0o700))?;
    assert!(result.is_err());
    assert_eq!(b.ledger()?.head, Some(first.clone()));
    b.history(&first)?;
    let backup = b.root.join("backups").join(&first);
    assert_eq!(
        sha256(&storage::read(
            &backup.join("manifest.json"),
            storage::MAX_METADATA
        )?),
        first
    );
    for f in b.manifest(&first)?.files {
        assert_eq!(
            sha256(&storage::read(
                &backup.join(&f.hash),
                harness_sync_core::MAX_FILE
            )?),
            f.hash
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_click_test_prepares_both_peers_requires_remote_adoption_and_blocks_duplicates(
) -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "Desktop").await;
    let b = engine(&t, "Laptop").await;
    // No fixture initialization or manual note creation on either computer.
    assert!(a
        .request("run_sync_test", json!({"device":b.identity.id}))
        .await
        .is_err());
    assert!(a.status().await?["head"].is_null());
    pair(&a, &b, true).await?;
    wait_for(&a, |s| s["transport"]["peers"][0]["connected"] == true).await;
    let (first, duplicate) = tokio::join!(
        a.request("run_sync_test", json!({"device":b.identity.id})),
        a.request("run_sync_test", json!({"device":b.identity.id}))
    );
    let result = first?;
    assert!(duplicate
        .unwrap_err()
        .to_string()
        .contains("already running"));
    assert_eq!(result["state"], "success");
    let remote = b.status().await?;
    assert_eq!(remote["head"], result["snapshot"]);
    assert!(
        remote["transfer"]["verified_received_objects"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        a.status().await?["transport"]["peers"][0]["receipt"]["head"],
        result["snapshot"]
    );
    assert_eq!(
        a.status().await?["transport"]["peers"][0]["receipt"]["received"],
        result["snapshot"]
    );
    assert_eq!(a.status().await?["sync_test"]["state"], "success");
    // A connected peer with an active writer must never yield a false success.
    storage::atomic(
        &t.path().join("Laptop/WRITER_ACTIVE"),
        b"quarantined test writer",
    )?;
    let refused = a
        .request("run_sync_test", json!({"device":b.identity.id}))
        .await?;
    assert_ne!(refused["state"], "success");
    assert_eq!(b.status().await?["head"], result["snapshot"]);
    for e in [&a, &b] {
        assert!(e.request("production_import", json!({})).await.is_err());
        assert!(e.request("production_export", json!({})).await.is_err());
        e.shutdown();
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cannot_claim_success_for_a_divergent_peer() -> Result<()> {
    let t = TempDir::new()?;
    let a = engine(&t, "Left").await;
    let b = engine(&t, "Right").await;
    init(&a).await;
    init(&b).await;
    let left = edit(&a, "Left offline version").await;
    let right = edit(&b, "Right offline version").await;
    assert_eq!(
        a.request("inspect_revision", json!({"snapshot":left}))
            .await?["text"],
        "Left offline version"
    );
    assert_eq!(
        b.request("inspect_revision", json!({"snapshot":right}))
            .await?["text"],
        "Right offline version"
    );
    assert!(a
        .request("inspect_revision", json!({"snapshot":"../../etc/passwd"}))
        .await
        .is_err());
    pair(&a, &b, true).await?;
    wait_for(&a, |s| !s["conflicts"].as_array().unwrap().is_empty()).await;
    assert!(a
        .request("run_sync_test", json!({"device":b.identity.id}))
        .await
        .is_err());
    assert_eq!(a.status().await?["head"], left);
    assert_eq!(b.status().await?["head"], right);
    assert_ne!(a.status().await?["sync_test"]["state"], "success");
    a.shutdown();
    b.shutdown();
    Ok(())
}
