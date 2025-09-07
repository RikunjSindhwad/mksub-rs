use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal};

/// Read base domains from various sources
pub fn read_domains(
    single_domain: Option<&str>,
    domain_file: Option<&str>,
) -> Result<Vec<String>> {
    let mut domains = Vec::new();

    // Handle single domain
    if let Some(domain) = single_domain {
        let trimmed = domain.trim();
        if !trimmed.is_empty() {
            domains.push(trimmed.to_string());
        }
    }

    // Handle domain file
    if let Some(path) = domain_file {
        let file = File::open(path)
            .with_context(|| format!("Failed to open domain file: {}", path))?;
        let reader = BufReader::new(file);
        
        for line in reader.lines() {
            let line = line.context("Failed to read line from domain file")?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                domains.push(trimmed.to_string());
            }
        }
    }

    // Handle stdin if no other sources
    if domains.is_empty() {
        if io::stdin().is_terminal() {
            anyhow::bail!("No domains provided and stdin is a TTY");
        }
        
        let stdin = io::stdin();
        let reader = stdin.lock();
        
        for line in reader.lines() {
            let line = line.context("Failed to read line from stdin")?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                domains.push(trimmed.to_string());
            }
        }
    }

    Ok(domains)
}

/// Read wordlist, apply normalization, deduplication, and optional regex filtering
pub fn read_wordlist(
    path: &str,
    regex_filter: Option<&str>,
    case_insensitive: bool,
) -> Result<Vec<String>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open wordlist file: {}", path))?;
    let reader = BufReader::new(file);

    // Compile regex if provided
    let regex = if let Some(pattern) = regex_filter {
        let mut builder = regex::RegexBuilder::new(pattern);
        builder.case_insensitive(case_insensitive);
        Some(
            builder
                .build()
                .with_context(|| format!("Failed to compile regex: {}", pattern))?
        )
    } else {
        None
    };

    let mut word_set = HashSet::new();
    let mut words = Vec::new();

    for line in reader.lines() {
        let line = line.context("Failed to read line from wordlist")?;
        let normalized = normalize_word(&line);
        
        if normalized.is_empty() {
            continue;
        }

        // Apply regex filter if provided
        if let Some(ref re) = regex {
            if !re.is_match(&normalized) {
                continue;
            }
        }

        // Deduplicate using HashSet
        if word_set.insert(normalized.clone()) {
            words.push(normalized);
        }
    }

    Ok(words)
}

/// Normalize a word: lowercase, trim leading/trailing dots and whitespace
fn normalize_word(word: &str) -> String {
    word.trim()
        .to_lowercase()
        .trim_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_normalize_word() {
        assert_eq!(normalize_word("  WORD  "), "word");
        assert_eq!(normalize_word(".word."), "word");
        assert_eq!(normalize_word("..WORD.."), "word");
        assert_eq!(normalize_word("  .Word.  "), "word");
        assert_eq!(normalize_word("...."), "");
        assert_eq!(normalize_word("   "), "");
    }

    #[test]
    fn test_read_wordlist_basic() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "API")?;
        writeln!(temp_file, "  cdn  ")?;
        writeln!(temp_file, ".img.")?;
        writeln!(temp_file, "api")?; // duplicate
        writeln!(temp_file, "")?; // empty
        writeln!(temp_file, "...")?; // only dots
        
        let words = read_wordlist(temp_file.path().to_str().unwrap(), None, true)?;
        
        assert_eq!(words.len(), 3);
        assert!(words.contains(&"api".to_string()));
        assert!(words.contains(&"cdn".to_string()));
        assert!(words.contains(&"img".to_string()));
        
        Ok(())
    }

    #[test]
    fn test_read_wordlist_with_regex() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "api")?;
        writeln!(temp_file, "cdn")?;
        writeln!(temp_file, "img")?;
        writeln!(temp_file, "test")?;
        
        let words = read_wordlist(
            temp_file.path().to_str().unwrap(), 
            Some("^(api|img)$"), 
            true
        )?;
        
        assert_eq!(words.len(), 2);
        assert!(words.contains(&"api".to_string()));
        assert!(words.contains(&"img".to_string()));
        assert!(!words.contains(&"cdn".to_string()));
        assert!(!words.contains(&"test".to_string()));
        
        Ok(())
    }

    #[test]
    fn test_read_domains_single() -> Result<()> {
        let domains = read_domains(Some("  example.com  "), None)?;
        assert_eq!(domains, vec!["example.com"]);
        Ok(())
    }

    #[test]
    fn test_read_domains_file() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "example.com")?;
        writeln!(temp_file, "  test.org  ")?;
        writeln!(temp_file, "")?; // empty line
        writeln!(temp_file, "domain.net")?;
        
        let domains = read_domains(None, Some(temp_file.path().to_str().unwrap()))?;
        
        assert_eq!(domains.len(), 3);
        assert!(domains.contains(&"example.com".to_string()));
        assert!(domains.contains(&"test.org".to_string()));
        assert!(domains.contains(&"domain.net".to_string()));
        
        Ok(())
    }
}
