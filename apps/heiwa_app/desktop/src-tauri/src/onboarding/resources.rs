//! Bounded, passive inventory. Discovery reads installation metadata only.
//! It never opens apps, reads personal databases, or grants capabilities.

use heiwa_config::HeiwaPaths;
use heiwa_provider::AccountRegistry;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Resource {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub app_detected: bool,
    pub tools_detected: Vec<&'static str>,
    pub registered_accounts: usize,
    pub detail: &'static str,
    pub surface: Option<&'static str>,
    pub has_guide: bool,
}

struct Descriptor {
    id: &'static str,
    name: &'static str,
    app: &'static str,
    category: &'static str,
    tools: &'static [&'static str],
    providers: &'static [&'static str],
    detail: &'static str,
    surface: Option<&'static str>,
    guide: Option<&'static str>,
}

impl Descriptor {
    const fn apple(
        id: &'static str,
        name: &'static str,
        app: &'static str,
        detail: &'static str,
        surface: Option<&'static str>,
    ) -> Self {
        Self {
            id,
            name,
            app,
            category: "apple",
            tools: &[],
            providers: &[],
            detail,
            surface,
            guide: None,
        }
    }
}

const CATALOG: &[Descriptor] = &[
    Descriptor { id: "openai", name: "ChatGPT / OpenAI", category: "inference", app: "ChatGPT.app", tools: &["codex"], providers: &["openai"], detail: "ChatGPT, Codex, and OpenAI API access are separate connections. App detection does not provide a credential or prove image generation access.", surface: None, guide: Some("https://developers.openai.com/codex/auth") },
    Descriptor { id: "anthropic", name: "Claude / Anthropic", category: "inference", app: "Claude.app", tools: &["claude"], providers: &["anthropic"], detail: "Claude, Claude Code, and Anthropic API access are separate connections. Visuals made with code and image generation through tools need different capability checks.", surface: None, guide: Some("https://code.claude.com/docs/en/authentication") },
    Descriptor { id: "google", name: "Google / Antigravity", category: "inference", app: "Antigravity.app", tools: &["agy", "gemini", "antigravity"], providers: &["google", "antigravity"], detail: "Antigravity, Gemini CLI, and Google API access are separate connections. Image tools depend on the selected account and execution channel.", surface: None, guide: Some("https://antigravity.google/docs/cli/install/") },
    Descriptor { id: "ollama", name: "Ollama", category: "inference", app: "Ollama.app", tools: &["ollama"], providers: &["ollama"], detail: "Local inference needs a running endpoint and installed models. Finding the app or CLI does not prove either is ready.", surface: None, guide: Some("https://docs.ollama.com/quickstart") },
    Descriptor { id: "openrouter", name: "OpenRouter", category: "inference", app: "", tools: &[], providers: &["openrouter"], detail: "Connect your own API account. Models, costs, and modalities must be checked for that account before routing.", surface: None, guide: Some("https://openrouter.ai/docs/quickstart") },
    Descriptor::apple("calendar", "Apple Calendar", "Calendar.app", "Connect in Calendar to list your calendars and events. Event changes are staged for review through the existing connector.", Some("calendar")),
    Descriptor::apple("mail", "Apple Mail", "Mail.app", "Open Mail in Heiwa and choose Read Apple Mail to import up to 50 inbox headers. macOS may request Automation access. Message bodies, sending, and message changes are not supported by this read-only connection.", Some("mail")),
    Descriptor::apple("reminders", "Reminders", "Reminders.app", "EventKit permission, list selection, and the Reminders connector are not yet available in this desktop build.", None),
    Descriptor::apple("notes", "Notes", "Notes.app", "A scoped Notes import and action connector is still required. No notes are read during discovery.", None),
    Descriptor::apple("contacts", "Contacts", "Contacts.app", "Contact access requires a connector and a separate macOS permission request.", None),
    Descriptor::apple("safari", "Safari", "Safari.app", "Tabs and browsing history require an implemented browser connector. App detection does not grant access.", None),
    Descriptor::apple("messages", "Messages", "Messages.app", "Conversation access and sending require a supported, separately authorized integration.", None),
    Descriptor::apple("photos", "Photos", "Photos.app", "Photo import requires a system picker or PhotoKit integration. This build does not import your library.", None),
    Descriptor::apple("music", "Music", "Music.app", "Library and playback integration need a Music adapter. These are separate from music generation.", None),
    Descriptor::apple("shortcuts", "Shortcuts", "Shortcuts.app", "Shortcuts are not imported or run here. The connector must expose each shortcut's inputs and effects.", None),
    Descriptor::apple("pages", "Pages", "Pages.app", "Document selection, editing, and export need a document adapter.", None),
    Descriptor::apple("numbers", "Numbers", "Numbers.app", "Spreadsheet selection, editing, and export need a document adapter.", None),
    Descriptor::apple("keynote", "Keynote", "Keynote.app", "Presentation selection, editing, and export need a document adapter.", None),
];

pub fn guide_for(id: &str) -> Option<&'static str> {
    CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.guide)
}

fn application_roots(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Cryptexes/App/System/Applications"),
    ];
    if let Some(home) = home {
        roots.push(home.join("Applications"));
    }
    roots
}

fn project(
    roots: &[PathBuf],
    app_exists: impl Fn(&Path) -> bool,
    tool_exists: impl Fn(&str) -> bool,
    account_count: impl Fn(&[&str]) -> usize,
) -> Vec<Resource> {
    CATALOG
        .iter()
        .map(|entry| Resource {
            id: entry.id,
            name: entry.name,
            category: entry.category,
            app_detected: !entry.app.is_empty()
                && roots.iter().any(|root| app_exists(&root.join(entry.app))),
            tools_detected: entry
                .tools
                .iter()
                .copied()
                .filter(|tool| tool_exists(tool))
                .collect(),
            registered_accounts: account_count(entry.providers),
            detail: entry.detail,
            surface: entry.surface,
            has_guide: entry.guide.is_some(),
        })
        .collect()
}

pub fn discover(paths: Option<&HeiwaPaths>, registry: &AccountRegistry) -> Vec<Resource> {
    project(
        &application_roots(paths.map(|paths| paths.home_dir.as_path())),
        |path| cfg!(target_os = "macos") && path.is_dir(),
        |tool| heiwa_provider::resolve_command(tool).is_some(),
        |providers| {
            registry
                .accounts
                .iter()
                .filter(|account| providers.contains(&account.provider.as_str()))
                .count()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_machine_has_no_invented_accounts_or_apps() {
        let rows = project(
            &application_roots(Some(Path::new("/fresh-user"))),
            |_| false,
            |_| false,
            |_| 0,
        );
        assert!(rows.iter().all(|row| !row.app_detected
            && row.tools_detected.is_empty()
            && row.registered_accounts == 0));
        assert_eq!(rows.len(), 18);
    }

    #[test]
    fn user_apps_and_clis_are_independent_of_account_registration() {
        let rows = project(
            &application_roots(Some(Path::new("/fresh-user"))),
            |path| path == Path::new("/fresh-user/Applications/Claude.app"),
            |tool| tool == "agy",
            |_| 0,
        );
        let claude = rows.iter().find(|r| r.id == "anthropic").unwrap();
        assert!(claude.app_detected);
        assert_eq!(claude.registered_accounts, 0);
        assert!(claude.tools_detected.is_empty());
        let google = rows.iter().find(|r| r.id == "google").unwrap();
        assert!(!google.app_detected);
        assert_eq!(google.tools_detected, vec!["agy"]);
        assert_eq!(google.registered_accounts, 0);
    }

    #[test]
    fn guide_opening_only_accepts_catalog_ids() {
        assert!(guide_for("openai").unwrap().starts_with("https://"));
        assert_eq!(guide_for("file:///private/data"), None);
        assert_eq!(guide_for("calendar"), None);
    }
}
