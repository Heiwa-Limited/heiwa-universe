use anyhow::{Context, Result};
use heiwa_paths::RuntimePaths;
use std::fs;
use std::path::Path;

pub fn seed_runtime(paths: &RuntimePaths, repo_root: &Path) -> Result<()> {
    let concise_mode_source = repo_root.join("packages/heiwa_skills/heiwa-concise-mode/MODE.md");
    let concise_mode = fs::read_to_string(&concise_mode_source)
        .with_context(|| format!("missing concise mode source: {}", concise_mode_source.display()))?;

    write_if_missing(&paths.concise_mode(), &concise_mode)?;
    write_if_missing(
        &paths.root().join("capabilities/research/manifest.json"),
        concat!(
            "{\n",
            "  \"id\": \"research\",\n",
            "  \"owner\": \"heiwa\",\n",
            "  \"mode\": \"concise\",\n",
            "  \"session_owner\": \"heiwa\",\n",
            "  \"sandbox_owner\": \"heiwa\",\n",
            "  \"provider_inference_owner\": \"provider\"\n",
            "}\n"
        ),
    )?;
    write_if_missing(
        &paths.root().join("capabilities/operator/manifest.json"),
        concat!(
            "{\n",
            "  \"id\": \"operator\",\n",
            "  \"owner\": \"heiwa\",\n",
            "  \"mode\": \"concise\",\n",
            "  \"session_owner\": \"heiwa\",\n",
            "  \"sandbox_owner\": \"heiwa\",\n",
            "  \"provider_inference_owner\": \"provider\"\n",
            "}\n"
        ),
    )?;
    write_if_missing(&paths.inventory(), "{\n  \"models\": []\n}\n")?;
    write_if_missing(
        &paths.runtime_policy(),
        concat!(
            "[ownership]\n",
            "providers = [\"auth\", \"inference\"]\n",
            "heiwa = [\"sessions\", \"sandboxes\"]\n\n",
            "[mode]\n",
            "default = \"concise\"\n",
        ),
    )?;

    Ok(())
}

fn write_if_missing(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
