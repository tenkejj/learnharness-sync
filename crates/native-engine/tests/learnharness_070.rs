use harness_sync_core::learnharness_070;
use harness_sync_native::{
    compatibility, learnharness_export::ExportManifest, managed_profile::Tree,
};
use std::path::PathBuf;

#[test]
#[ignore = "requires a real LearnHarness 0.7.0 run in a disposable qualification profile"]
fn real_070_text_graph_profile_matches_the_existing_strict_subset() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let allowed = repo
        .join("build/qualification/learnharness-070")
        .canonicalize()
        .unwrap();
    let data = PathBuf::from(
        std::env::var("LEARNHARNESS_070_DISPOSABLE_DATA")
            .expect("Supply a real 0.7.0 disposable qualification data directory"),
    )
    .canonicalize()
    .unwrap();
    assert!(
        data.starts_with(&allowed),
        "Refusing a non-disposable profile"
    );
    assert!(data.ends_with("config/LearnHarness/data"));
    let tree = Tree::read(&data).expect("Real 0.7.0 durable inventory must be understood");
    assert!(tree.files.contains_key("settings.json"));
    let records = Tree::records(&data).unwrap();
    let has_folders = records.contains_key("graph/source-organization.json");
    assert_eq!(records.len(), if has_folders { 7 } else { 6 });
    assert!(records.values().map(Vec::len).sum::<usize>() >= 4_932);
    let manifest =
        ExportManifest::from_payload_for_format(&records, learnharness_070::FORMAT).unwrap();
    assert_eq!(manifest.format, learnharness_070::FORMAT);
    harness_sync_native::learnharness_export::verify(&manifest, &records).unwrap();
    let chat = records
        .iter()
        .find(|(path, _)| path.starts_with("chats/"))
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(chat.1).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["messages"].as_array().unwrap().len(), 4);
    if has_folders {
        let mut invalid = records.clone();
        let key = "graph/source-organization.json";
        let mut organization: serde_json::Value = serde_json::from_slice(&records[key]).unwrap();
        assert_eq!(organization["version"], 1);
        assert_eq!(organization["folders"].as_array().unwrap().len(), 1);
        assert_eq!(organization["placements"].as_object().unwrap().len(), 1);

        let id = organization["folders"][0]["id"].clone();
        organization["folders"][0]["parentId"] = id;
        invalid.insert(key.into(), serde_json::to_vec(&organization).unwrap());
        assert!(learnharness_070::validate(&invalid).is_err());

        organization["folders"][0]["parentId"] = serde_json::Value::Null;
        organization["folders"][0]["importPath"] = serde_json::json!("private/notes");
        invalid.insert(key.into(), serde_json::to_vec(&organization).unwrap());
        assert!(learnharness_070::validate(&invalid).is_err());
    }
}

#[test]
#[ignore = "requires the official 0.7.0 AppImage in an ignored disposable qualification directory"]
fn verified_image_survives_an_untrusted_upstream_update() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let source =
        repo.join("build/qualification/learnharness-070/LearnHarness-0.7.0-x86_64.AppImage");
    let selected = compatibility::matrix()
        .builds
        .iter()
        .find(|build| build.version == "0.7.0")
        .unwrap();
    compatibility::verify_image(&source, selected).unwrap();
    let temp =
        tempfile::TempDir::new_in(repo.join("build/qualification/learnharness-070")).unwrap();
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    std::fs::create_dir_all(home.join("Applications")).unwrap();
    let installed = home.join("Applications/LearnHarness-0.7.0-x86_64.AppImage");
    std::fs::copy(&source, &installed).unwrap();
    let first = compatibility::detect(&home, &data).unwrap();
    assert_eq!(first.build.version, "0.7.0");
    let pinned = compatibility::retain_verified_image(&first, &data).unwrap();
    std::fs::remove_file(installed).unwrap();
    std::fs::write(
        home.join("Applications/LearnHarness-0.8.0-x86_64.AppImage"),
        b"unknown",
    )
    .unwrap();
    let after = compatibility::detect(&home, &data).unwrap();
    assert_eq!(after.build.version, "0.7.0");
    assert_eq!(after.newer_unverified.as_deref(), Some("0.8.0"));
    assert_eq!(after.source, pinned);
    assert_eq!(std::fs::metadata(pinned).unwrap().len(), selected.size);
}
