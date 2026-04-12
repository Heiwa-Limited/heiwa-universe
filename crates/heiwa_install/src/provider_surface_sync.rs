use anyhow::{Context, Result};
use heiwa_paths::RuntimePaths;
use std::fs;
use std::path::Path;

const MODE_ID: &str = "heiwa-concise-mode";
const BEGIN_MARKER: &str = "<!-- HEIWA:BEGIN concise-context -->";
const END_MARKER: &str = "<!-- HEIWA:END concise-context -->";

pub fn sync_surfaces(paths: &RuntimePaths, repo_root: &Path) -> Result<()> {
    let skill_source = repo_root.join("packages/heiwa_skills").join(MODE_ID);
    if !skill_source.join("SKILL.md").exists() {
        anyhow::bail!("missing concise mode source at {}", skill_source.display());
    }

    let home = paths
        .root()
        .parent()
        .context("runtime root missing parent home directory")?;

    let codex_context = fs::read_to_string(paths.root().join("generated/codex/AGENTS.md"))?;
    let claude_context = fs::read_to_string(paths.root().join("generated/claude/CLAUDE.md"))?;
    let gemini_context = fs::read_to_string(paths.root().join("generated/gemini/GEMINI.md"))?;
    let antigravity_context =
        fs::read_to_string(paths.root().join("generated/antigravity/GEMINI.md"))?;

    copy_dir_replace(&skill_source, &home.join(".codex/skills").join(MODE_ID))?;
    copy_dir_replace(&skill_source, &home.join(".claude/skills").join(MODE_ID))?;
    sync_startup_context(&home.join(".codex/AGENTS.md"), &codex_context)?;
    sync_startup_context(&home.join(".claude/CLAUDE.md"), &claude_context)?;
    sync_startup_context(&home.join(".gemini/GEMINI.md"), &gemini_context)?;
    sync_startup_context(
        &home.join(".gemini/antigravity/GEMINI.md"),
        &antigravity_context,
    )?;

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
        format!("{gemini_context}\n\n@./skills/heiwa-concise-mode/SKILL.md\n"),
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

fn sync_startup_context(path: &Path, context: &str) -> Result<()> {
    let managed_block = format!("{BEGIN_MARKER}\n{context}\n{END_MARKER}\n");
    let existing = fs::read_to_string(path).unwrap_or_default();
    let updated = if let Some((start, end)) = managed_block_range(&existing) {
        format!("{}{}{}", &existing[..start], managed_block, &existing[end..])
    } else if existing.trim().is_empty() {
        managed_block
    } else if let Some(insert_at) = insertion_after_first_heading(&existing) {
        format!(
            "{}\n\n{}{}",
            &existing[..insert_at],
            managed_block,
            &existing[insert_at..]
        )
    } else {
        format!("{managed_block}\n{existing}")
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, updated)?;
    Ok(())
}

fn managed_block_range(content: &str) -> Option<(usize, usize)> {
    let start = content.find(BEGIN_MARKER)?;
    let end_marker = content.find(END_MARKER)?;
    let end = end_marker + END_MARKER.len();
    let end = if content[end..].starts_with('\n') {
        end + 1
    } else {
        end
    };
    Some((start, end))
}

fn insertion_after_first_heading(content: &str) -> Option<usize> {
    let first_line_end = content.find('\n')?;
    if !content.starts_with('#') {
        return None;
    }
    Some(first_line_end + 1)
}
