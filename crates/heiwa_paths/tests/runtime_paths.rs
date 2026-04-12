use std::path::PathBuf;

use heiwa_paths::RuntimePaths;

#[test]
fn runtime_root_lives_under_hidden_heiwa_dir() {
    let paths = RuntimePaths::from_home(PathBuf::from("/tmp/heiwa-user"));
    assert_eq!(paths.root(), PathBuf::from("/tmp/heiwa-user/.heiwa").as_path());
}

#[test]
fn registry_and_state_paths_use_structured_runtime_layout() {
    let paths = RuntimePaths::from_home(PathBuf::from("/tmp/heiwa-user"));

    assert_eq!(
        paths.provider_registry(),
        PathBuf::from("/tmp/heiwa-user/.heiwa/providers/registry.json")
    );
    assert_eq!(
        paths.identity(),
        PathBuf::from("/tmp/heiwa-user/.heiwa/state/identity.json")
    );
    assert_eq!(
        paths.connection(),
        PathBuf::from("/tmp/heiwa-user/.heiwa/state/connection.json")
    );
}
