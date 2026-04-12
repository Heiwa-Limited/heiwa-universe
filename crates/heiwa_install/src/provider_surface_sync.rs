use anyhow::{Context, Result};
use heiwa_paths::RuntimePaths;
use std::fs;
use std::path::Path;

const MODE_ID: &str = "heiwa-concise-mode";

pub fn sync_surfaces(paths: &RuntimePaths, repo_root: &Path) -> Result<()> {
    let skill_source = repo_root.join("packages/heiwa_skills").join(MODE_ID);
    if !skill_source.join("SKILL.md").exists() {
        anyhow::bail!("missing concise mode source at {}", skill_source.display());
    }

    let home = paths
        .root()
        .parent()
        .context("runtime root missing parent home directory")?;

    copy_dir_replace(&skill_source, &home.join(".codex/skills").join(MODE_ID))?;
    copy_dir_replace(&skill_source, &home.join(".claude/skills").join(MODE_ID))?;

    let gemini_extension_root = home.join(".gemini/extensions").join(MODE_ID);
    remove_target(&gemini_extension_root)?;
    fs::create_dir_all(&gemini_extension_root)?;
    fs::write(
        gemini_extension_root.join("gemini-extension.json"),
        concat!(
            "{\n",
            "  \"name\": \"heiwa-concise-mode\",\n",
            "  \"description\": \"Provider-agnostic concise response mode for Heiwa operator work\",\n",
            "  \"version\": \"0.1.0\",\n",
            "  \"contextFileName\": \"GEMINI.md\"\n",
            "}\n"
        ),
    )?;
    fs::write(
        gemini_extension_root.join("GEMINI.md"),
        "@./skills/heiwa-concise-mode/SKILL.md\n",
    )?;
    copy_dir_replace(
        &skill_source,
        &gemini_extension_root.join("skills").join(MODE_ID),
    )?;

    let heiwa_mode_root = paths.root().join("modes").join(MODE_ID);
    fs::create_dir_all(&heiwa_mode_root)?;
    fs::write(
        heiwa_mode_root.join("manifest.json"),
        concat!(
            "{\n",
            "  \"id\": \"heiwa-concise-mode\",\n",
            "  \"name\": \"Heiwa Concise Mode\",\n",
            "  \"version\": \"0.1.0\",\n",
            "  \"provider_agnostic\": true,\n",
            "  \"model_agnostic\": true,\n",
            "  \"targets\": [\"codex\", \"claude\", \"gemini\", \"antigravity\", \"heiwa\", \"ollama\"],\n",
            "  \"upstream\": {\n",
            "    \"repo\": \"https://github.com/JuliusBrussee/caveman\",\n",
            "    \"release\": \"v1.3.5\"\n",
            "  }\n",
            "}\n"
        ),
    )?;
    fs::copy(skill_source.join("MODE.md"), heiwa_mode_root.join("MODE.md"))?;
    fs::copy(skill_source.join("README.md"), heiwa_mode_root.join("README.md"))?;

    Ok(())
}

fn copy_dir_replace(source: &Path, target: &Path) -> Result<()> {
    remove_target(target)?;
    copy_dir_recursive(source, target)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn remove_target(path: &Path) -> Result<()> {
    if !path.exists() && !path.is_symlink() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
