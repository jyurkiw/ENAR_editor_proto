//! The project word list: `enigma-words.txt` next to the project's
//! `enigma-project.toml`, holding words the spellchecker should accept (names, game
//! terms). One word per line, `#` lines are comments; kept sorted so git diffs stay
//! small.

use std::path::{Path, PathBuf};

use shared::{AppError, ProjectInfo};

pub const FILE: &str = "enigma-words.txt";

const HEADER: &str = "# Words the Enigma editor's spellchecker accepts in this project, one per line.\n# Added from a prose box's right-click menu (\"Add to project words\"); edit freely.\n";

pub fn path(project: &ProjectInfo) -> PathBuf {
    Path::new(&project.root).join(FILE)
}

/// The words in `text` (comments and blank lines skipped).
fn parse(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect()
}

/// The project's words; none if the file doesn't exist yet.
pub fn read(project: &ProjectInfo) -> Vec<String> {
    std::fs::read_to_string(path(project)).map(|t| parse(&t)).unwrap_or_default()
}

/// The file's text with `word` added: its leading comments kept, the words sorted
/// (ignoring case) without duplicates.
fn with_word(text: Option<&str>, word: &str) -> String {
    let header: String = match text {
        Some(t) => t.lines().take_while(|l| l.trim().starts_with('#') || l.trim().is_empty()).filter(|l| l.trim().starts_with('#')).map(|l| format!("{l}\n")).collect(),
        None => HEADER.to_string(),
    };
    let mut words = text.map(parse).unwrap_or_default();
    if !words.iter().any(|w| w == word) {
        words.push(word.to_string());
    }
    words.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
    words.dedup();
    let mut out = header;
    for w in words {
        out.push_str(&w);
        out.push('\n');
    }
    out
}

/// Adds `word` to the list (creating the file if need be) and returns the new list.
pub fn add(project: &ProjectInfo, word: &str) -> Result<Vec<String>, AppError> {
    let word = word.trim();
    if word.is_empty() || word.chars().any(char::is_whitespace) || word.starts_with('#') {
        return Err(AppError::new(format!("\u{201c}{word}\u{201d} isn't a single word")));
    }
    let p = path(project);
    let old = std::fs::read_to_string(&p).ok();
    let text = with_word(old.as_deref(), word);
    std::fs::write(&p, &text).map_err(|e| AppError::new(format!("Can't write {}: {e}", p.display())))?;
    Ok(parse(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(dir: &Path) -> ProjectInfo {
        ProjectInfo { root: dir.to_string_lossy().into(), name: "t".into(), schemas_dir: String::new(), compiler_dir: String::new() }
    }

    #[test]
    fn add_creates_sorts_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        let p = project(dir.path());
        assert!(read(&p).is_empty());
        assert_eq!(add(&p, "Yeenoghu").unwrap(), ["Yeenoghu"]);
        assert_eq!(add(&p, "gnolls").unwrap(), ["gnolls", "Yeenoghu"]);
        assert_eq!(add(&p, "Yeenoghu").unwrap(), ["gnolls", "Yeenoghu"]);
        assert_eq!(add(&p, "brewhouse").unwrap(), ["brewhouse", "gnolls", "Yeenoghu"]);
        let text = std::fs::read_to_string(path(&p)).unwrap();
        assert!(text.starts_with("# Words the Enigma editor"), "{text}");
        assert!(text.ends_with("brewhouse\ngnolls\nYeenoghu\n"), "{text}");
        assert!(add(&p, "two words").is_err());
        assert!(add(&p, " ").is_err());
    }

    #[test]
    fn keeps_a_hand_written_header() {
        let dir = tempfile::tempdir().unwrap();
        let p = project(dir.path());
        std::fs::write(path(&p), "# mine\n\nzeta\nalpha\n# stray comment\n").unwrap();
        assert_eq!(read(&p), ["zeta", "alpha"]);
        add(&p, "Mid").unwrap();
        assert_eq!(std::fs::read_to_string(path(&p)).unwrap(), "# mine\nalpha\nMid\nzeta\n");
    }
}
