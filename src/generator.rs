use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Generate all subdomain combinations for a base domain up to specified level
pub fn generate_subdomains<F>(
    base_domain: &str,
    words: &[String],
    max_level: u32,
    threads: usize,
    emit: F,
)
where
    F: Fn(String) + Sync + Send,
{
    if max_level == 0 || words.is_empty() {
        return;
    }

    // Parallelize over first word (w1) using rayon
    words
        .par_iter()
        .with_max_len(if threads > 0 { 
            std::cmp::max(1, words.len() / threads) 
        } else { 
            1 
        })
        .for_each(|w1| {
            if SHUTDOWN.load(Ordering::Relaxed) {
                return;
            }

            // Start with level 1: w1.base
            generate_combinations(base_domain, words, vec![w1], 1, max_level, &emit);
        });
}

/// Recursively generate combinations for all levels
fn generate_combinations<F>(
    base_domain: &str,
    words: &[String], 
    current_chain: Vec<&String>,
    current_level: u32,
    max_level: u32,
    emit: &F,
)
where
    F: Fn(String) + Sync + Send,
{
    if current_level > max_level || SHUTDOWN.load(Ordering::Relaxed) {
        return;
    }
    
    // Emit current combination: chain[n-1].chain[n-2]...chain[0].base
    let subdomain = format!(
        "{}.{}",
        current_chain
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("."),
        base_domain
    );
    emit(subdomain);
    
    // Generate next level if not at max
    if current_level < max_level {
        for word in words {
            if SHUTDOWN.load(Ordering::Relaxed) {
                return;
            }
            
            let mut next_chain = Vec::with_capacity(current_chain.len() + 1);
            next_chain.push(word);
            next_chain.extend_from_slice(&current_chain);
            
            generate_combinations(base_domain, words, next_chain, current_level + 1, max_level, emit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_generate_level_1() {
        let words = vec!["api".to_string(), "cdn".to_string()];
        let results = Mutex::new(Vec::new());
        
        let emit = |line: String| {
            results.lock().unwrap().push(line);
        };

        generate_subdomains("example.com", &words, 1, 10, emit);
        
        let mut results = results.into_inner().unwrap();
        results.sort();
        
        assert_eq!(results, vec![
            "api.example.com",
            "cdn.example.com",
        ]);
    }

    #[test]
    fn test_generate_level_2() {
        let words = vec!["x".to_string(), "y".to_string()];
        let results = Mutex::new(Vec::new());
        
        let emit = |line: String| {
            results.lock().unwrap().push(line);
        };

        generate_subdomains("example.com", &words, 2, 10, emit);
        
        let mut results = results.into_inner().unwrap();
        results.sort();
        
        // Should include both level 1 and level 2
        let expected = vec![
            "x.example.com",
            "x.x.example.com", 
            "x.y.example.com",
            "y.example.com",
            "y.x.example.com",
            "y.y.example.com",
        ];
        
        assert_eq!(results, expected);
    }

    #[test]
    fn test_generate_level_3() {
        let words = vec!["a".to_string(), "b".to_string()];
        let results = Mutex::new(Vec::new());
        
        let emit = |line: String| {
            results.lock().unwrap().push(line);
        };

        generate_subdomains("test.com", &words, 3, 10, emit);
        
        let results = results.into_inner().unwrap();
        
        // Should have 2^1 + 2^2 + 2^3 = 2 + 4 + 8 = 14 combinations
        assert_eq!(results.len(), 14);
        
        // Check that we have all levels
        // Count dots in subdomain part (before .test.com)
        let level_1_count = results.iter().filter(|s| {
            let parts: Vec<&str> = s.split('.').collect();
            parts.len() == 3 // e.g., "a.test.com"
        }).count();
        let level_2_count = results.iter().filter(|s| {
            let parts: Vec<&str> = s.split('.').collect();
            parts.len() == 4 // e.g., "a.b.test.com"
        }).count(); 
        let level_3_count = results.iter().filter(|s| {
            let parts: Vec<&str> = s.split('.').collect();
            parts.len() == 5 // e.g., "a.b.c.test.com"
        }).count();
        
        assert_eq!(level_1_count, 2); // a.test.com, b.test.com
        assert_eq!(level_2_count, 4); // a.a.test.com, a.b.test.com, etc.
        assert_eq!(level_3_count, 8); // a.a.a.test.com, etc.
    }

    #[test]
    fn test_empty_words() {
        let words: Vec<String> = vec![];
        let results = Mutex::new(Vec::new());
        
        let emit = |line: String| {
            results.lock().unwrap().push(line);
        };

        generate_subdomains("example.com", &words, 2, 10, emit);
        
        let results = results.into_inner().unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_level_0() {
        let words = vec!["api".to_string()];
        let results = Mutex::new(Vec::new());
        
        let emit = |line: String| {
            results.lock().unwrap().push(line);
        };

        generate_subdomains("example.com", &words, 0, 10, emit);
        
        let results = results.into_inner().unwrap();
        assert_eq!(results.len(), 0);
    }
}
