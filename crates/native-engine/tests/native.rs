use anyhow::Result;
use harness_sync_core::{sha256, Decision, Manifest};
use harness_sync_native::{
    adapter,
    engine::{Engine, Peer},
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
