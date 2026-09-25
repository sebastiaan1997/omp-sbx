use std::collections::BTreeSet;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Model { pub selector: String, pub provider: String, pub name: String, pub reasoning: bool, pub thinking: Option<Vec<String>> }
#[derive(Debug, Deserialize)] struct Catalog { models: Vec<Model> }

pub fn parse_catalog(input: &str) -> Result<Vec<Model>> {
    let catalog: Catalog = serde_json::from_str(input).context("parse models output")?;
    if catalog.models.is_empty() { bail!("malformed models output: .models must be a non-empty array"); }
    let mut models = catalog.models;
    for (index, model) in models.iter().enumerate() {
        if model.selector.is_empty() || model.provider.is_empty() || model.name.is_empty() { bail!("malformed models output: .models[{index}] has an empty identity field"); }
        if let Some(levels) = &model.thinking {
            let mut unique = BTreeSet::new();
            if !model.reasoning || levels.iter().any(|level| !EFFORTS.contains(&level.as_str())) || !levels.iter().all(|level| unique.insert(level)) { bail!("malformed models output: .models[{index}].thinking is invalid"); }
        }
    }
    models.sort_by(|a, b| (&a.provider, &a.name, &a.selector).cmp(&(&b.provider, &b.name, &b.selector)));
    Ok(models)
}

pub fn normalize_selector(input: &str, models: &[Model]) -> Result<(String, String)> {
    let base = input.split(':').next().unwrap_or(input);
    if !models.iter().any(|model| model.selector == input || model.selector == base) { bail!("invalid model selector: {input}"); }
    let level = input.strip_prefix(base).and_then(|value| value.strip_prefix(':')).unwrap_or("inherit");
    if level != "inherit" && level != "off" && !EFFORTS.contains(&level) { bail!("invalid thinking level: {level}"); }
    let model = models.iter().find(|model| model.selector == base).unwrap();
    if level != "inherit" && level != "off" && !model.thinking.as_ref().is_some_and(|levels| levels.iter().any(|candidate| candidate == level)) { bail!("thinking level {level} is not supported by {base}"); }
    if level == "inherit" { Ok((base.to_owned(), base.to_owned())) } else { Ok((format!("{base}:{level}"), base.to_owned())) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigSnapshot { pub roles: serde_json::Map<String, serde_json::Value>, pub agents: serde_json::Map<String, serde_json::Value>, pub model_role_storage: String }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntendedChanges { pub roles: serde_json::Map<String, serde_json::Value>, pub agents: serde_json::Map<String, serde_json::Value>, #[serde(rename = "modelRoleStorage")] pub model_role_storage: String }

pub fn intended_changes(saved: &ConfigSnapshot, fast: &str, standard: &str, deep: &str, optional: &[(&str, Option<&str>)]) -> IntendedChanges {
    let mut roles = serde_json::Map::from_iter([("smol".to_owned(), fast.into()), ("default".to_owned(), standard.into()), ("plan".to_owned(), standard.into()), ("slow".to_owned(), deep.into()), ("advisor".to_owned(), deep.into()), ("task".to_owned(), saved.roles.get("task").cloned().unwrap_or_else(|| serde_json::Value::String("@default".to_owned()))) ]);
    let mut agents = serde_json::Map::from_iter([("scout".to_owned(), "@smol".into()), ("sonic".to_owned(), "@smol".into()), ("task".to_owned(), "@task".into()), ("reviewer".to_owned(), "@slow".into()), ("security-reviewer".to_owned(), "@slow".into())]);
    for (role, selector) in optional { if let Some(selector) = selector.filter(|value| !value.is_empty()) { roles.insert((*role).to_owned(), (*selector).into()); if *role == "designer" { agents.insert((*role).to_owned(), "@designer".into()); } } }
    IntendedChanges { roles, agents, model_role_storage: "project".to_owned() }
}

pub fn merged_config(saved: &ConfigSnapshot, changes: &IntendedChanges) -> ConfigSnapshot {
    let mut roles = saved.roles.clone(); roles.extend(changes.roles.clone());
    let mut agents = saved.agents.clone(); agents.extend(changes.agents.clone());
    ConfigSnapshot { roles, agents, model_role_storage: changes.model_role_storage.clone() }
}

pub fn publish_config(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    let parent = destination.parent().context("configuration path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut std::fs::File::open(source)?, &mut temporary)?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; temporary.as_file().set_permissions(std::fs::Permissions::from_mode(0o600))?; }
    temporary.as_file().sync_all()?;
    temporary.persist(destination).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn catalog_is_sorted_and_selector_is_validated() { let models = parse_catalog(r#"{"models":[{"selector":"b","provider":"p","name":"B","reasoning":true,"thinking":["low"]},{"selector":"a","provider":"p","name":"A","reasoning":true,"thinking":["high"]}]}"#).unwrap(); assert_eq!(models[0].selector, "a"); assert_eq!(normalize_selector("b:low", &models).unwrap().0, "b:low"); assert!(normalize_selector("b:high", &models).is_err()); }
    #[test] fn merges_saved_unrelated_roles() { let saved = ConfigSnapshot { roles: serde_json::Map::from_iter([(String::from("custom"), "x".into())]), agents: serde_json::Map::new(), model_role_storage: String::from("global") }; let changes = intended_changes(&saved, "a", "b", "c", &[]); let merged = merged_config(&saved, &changes); assert_eq!(merged.roles["custom"], "x"); assert_eq!(merged.roles["default"], "b"); assert_eq!(merged.model_role_storage, "project"); }
    #[test]
    fn optional_role_selections_update_roles_and_designer_agent() {
        let saved = ConfigSnapshot { roles: serde_json::Map::new(), agents: serde_json::Map::new(), model_role_storage: "global".into() };
        let changes = intended_changes(&saved, "fast", "standard", "deep", &[("designer", Some("designer-model")), ("tiny", Some("tiny-model"))]);
        assert_eq!(changes.roles["designer"], "designer-model");
        assert_eq!(changes.roles["tiny"], "tiny-model");
        assert_eq!(changes.agents["designer"], "@designer");
    }

    #[test]
    fn skipped_optional_role_does_not_create_assignment() {
        let saved = ConfigSnapshot { roles: serde_json::Map::new(), agents: serde_json::Map::new(), model_role_storage: "global".into() };
        let changes = intended_changes(&saved, "fast", "standard", "deep", &[("vision", None)]);
        assert!(!changes.roles.contains_key("vision"));
    }
}
