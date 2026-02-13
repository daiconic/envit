use std::{collections::HashMap, fs, io::Write, path::Path};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Add,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub key: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub(crate) enum Line {
    Raw(String),
    Entry(EntryLine),
}

#[derive(Debug, Clone)]
pub(crate) struct EntryLine {
    key: String,
    prefix: String,
    value: String,
}

fn parse_line(line: &str) -> Line {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Line::Raw(line.to_string());
    }

    let Some(eq_idx) = line.find('=') else {
        return Line::Raw(line.to_string());
    };

    let lhs = &line[..eq_idx];
    let key = lhs.trim();
    if !is_valid_env_key(key) {
        return Line::Raw(line.to_string());
    }

    let prefix = format!("{}=", lhs);
    let value = line[eq_idx + 1..].to_string();
    Line::Entry(EntryLine {
        key: key.to_string(),
        prefix,
        value,
    })
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

pub fn load_for_merge(path: &Path, create_if_missing: bool) -> Result<Vec<Line>> {
    if !path.exists() {
        if create_if_missing {
            return Ok(Vec::new());
        }
        bail!("env file does not exist: {}", path.display());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read env file: {}", path.display()))?;
    Ok(content.lines().map(parse_line).collect())
}

pub fn merge(lines: Vec<Line>, updates: &HashMap<String, String>) -> (String, Vec<Change>) {
    let mut remaining = updates.clone();
    let mut out_lines = Vec::with_capacity(lines.len() + remaining.len());
    let mut changes = Vec::new();

    for line in lines {
        match line {
            Line::Raw(raw) => out_lines.push(raw),
            Line::Entry(entry) => {
                if let Some(new_value) = remaining.remove(&entry.key) {
                    if new_value != entry.value {
                        changes.push(Change {
                            key: entry.key.clone(),
                            kind: ChangeKind::Update,
                        });
                    }
                    out_lines.push(format!("{}{}", entry.prefix, new_value));
                } else {
                    out_lines.push(format!("{}{}", entry.prefix, entry.value));
                }
            }
        }
    }

    let mut added: Vec<_> = remaining.into_iter().collect();
    added.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in added {
        changes.push(Change {
            key: key.clone(),
            kind: ChangeKind::Add,
        });
        out_lines.push(format!("{}={}", key, value));
    }

    (out_lines.join("\n"), changes)
}

pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let dir = path
        .parent()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    let mut tmp = NamedTempFile::new_in(&dir)
        .with_context(|| format!("failed to create temp file in {}", dir.display()))?;
    tmp.write_all(content.as_bytes())
        .context("failed to write temp env content")?;
    tmp.write_all(b"\n")
        .context("failed to finalize temp env content")?;
    tmp.flush().context("failed to flush temp env content")?;

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("failed to replace env file atomically: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_order_and_updates_targeted_keys() {
        let lines = vec![
            parse_line("# header"),
            parse_line("A=1"),
            parse_line("B=2"),
            parse_line(""),
            parse_line("LOCAL_ONLY=x"),
        ];
        let updates = HashMap::from([
            ("A".to_string(), "10".to_string()),
            ("C".to_string(), "3".to_string()),
        ]);

        let (merged, changes) = merge(lines, &updates);

        assert!(merged.contains("# header\nA=10\nB=2\n\nLOCAL_ONLY=x"));
        assert!(merged.contains("C=3"));
        assert_eq!(changes.len(), 2);
    }
}
