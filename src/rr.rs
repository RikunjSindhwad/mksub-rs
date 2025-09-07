use anyhow::Result;
use colored::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Round-robin selector for writer shards
pub struct RoundRobin {
    senders: Vec<Sender<String>>,
    counter: AtomicUsize,
}

impl RoundRobin {
    pub fn new(senders: Vec<Sender<String>>) -> Self {
        Self {
            senders,
            counter: AtomicUsize::new(0),
        }
    }

    pub fn next(&self) -> &Sender<String> {
        let index = self.counter.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        &self.senders[index]
    }
}

/// Initialize writer threads and return sender for round-robin distribution
pub fn init_writers(
    output_path: Option<&str>,
    shards: usize,
    buffer_mb: usize,
    queue_size: usize,
    silent: bool,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(Sender<String>, Vec<JoinHandle<()>>)> {
    let (main_sender, main_receiver) = bounded(queue_size);
    let mut writer_handles = Vec::new();
    let mut shard_senders = Vec::new();

    // Create writer shards
    for shard_id in 0..shards {
        let (shard_sender, shard_receiver) = bounded(queue_size);
        shard_senders.push(shard_sender);

        let output_file = output_path.map(|path| generate_shard_filename(path, shard_id, shards));

        let handle = spawn_writer_thread(
            shard_id,
            shard_receiver,
            output_file,
            buffer_mb,
            silent,
            shutdown_flag.clone(),
        )?;

        writer_handles.push(handle);
    }

    // Create round-robin distributor
    let rr = Arc::new(RoundRobin::new(shard_senders));
    let rr_clone = rr.clone();

    // Spawn distributor thread
    let distributor_handle = thread::spawn(move || {
        while let Ok(line) = main_receiver.recv() {
            if rr_clone.next().send(line).is_err() {
                // Shard receiver closed, stop distributing
                break;
            }
        }
        
        // Close all shard channels when main channel closes
        drop(rr_clone);
    });

    writer_handles.push(distributor_handle);

    Ok((main_sender, writer_handles))
}

/// Generate filename for a shard
fn generate_shard_filename(base_path: &str, shard_id: usize, total_shards: usize) -> String {
    let path = Path::new(base_path);
    
    // Add .txt extension if none exists
    let path_with_ext = if path.extension().is_none() {
        format!("{}.txt", base_path)
    } else {
        base_path.to_string()
    };

    if total_shards == 1 {
        path_with_ext
    } else {
        let path_obj = Path::new(&path_with_ext);
        let stem = path_obj.file_stem().unwrap().to_str().unwrap();
        let ext = path_obj.extension().map(|e| e.to_str().unwrap()).unwrap_or("txt");
        let parent = path_obj.parent().map(|p| p.to_str().unwrap()).unwrap_or("");
        
        if parent.is_empty() {
            format!("{}-{}.{}", stem, shard_id, ext)
        } else {
            format!("{}\\{}-{}.{}", parent, stem, shard_id, ext)
        }
    }
}

/// Spawn a writer thread for a shard
fn spawn_writer_thread(
    shard_id: usize,
    receiver: Receiver<String>,
    output_file: Option<String>,
    buffer_mb: usize,
    silent: bool,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let handle = thread::spawn(move || {
        let mut writer: Box<dyn Write + Send> = if let Some(ref path) = output_file {
            match File::create(path) {
                Ok(file) => {
                    let buf_size = buffer_mb * 1024 * 1024;
                    Box::new(BufWriter::with_capacity(buf_size, file))
                }
                Err(e) => {
                    eprintln!("{} {}: Failed to create output file '{}': {}", 
                             "❌".red(), 
                             format!("Shard {}", shard_id).bright_yellow(),
                             path.bright_cyan(), 
                             e);
                    return;
                }
            }
        } else {
            // Use a dummy writer that does nothing when writing to file only
            Box::new(std::io::sink())
        };

        let mut bytes_written = 0usize;
        let flush_threshold = buffer_mb * 1024 * 1024;
        
        loop {
            // Check shutdown flag first
            if shutdown_flag.load(Ordering::Relaxed) {
                break;
            }
            
            // Use timeout to avoid blocking indefinitely
            let line = match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(line) => line,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            };
            
            // Write to file if output_file is specified
            if output_file.is_some() {
                if let Err(e) = writeln!(writer, "{}", line) {
                    eprintln!("{} {}: Write error: {}", 
                             "❌".red(), 
                             format!("Shard {}", shard_id).bright_yellow(), 
                             e);
                    break;
                }
                bytes_written += line.len() + 1; // +1 for newline
            }

            // Write to stdout unless silent
            if !silent {
                // Add subtle coloring to generated subdomains
                let colored_line = if line.contains('.') {
                    let parts: Vec<&str> = line.split('.').collect();
                    if parts.len() >= 2 {
                        let subdomain_parts = &parts[..parts.len()-2];
                        let domain_parts = &parts[parts.len()-2..];
                        format!("{}.{}", 
                               subdomain_parts.join(".").bright_blue(), 
                               domain_parts.join(".").white())
                    } else {
                        line.bright_blue().to_string()
                    }
                } else {
                    line.to_string()
                };
                println!("{}", colored_line);
                // Force flush stdout to prevent mixing with stderr
                let _ = io::stdout().flush();
            }

            // Flush if threshold reached
            if bytes_written >= flush_threshold {
                if let Err(e) = writer.flush() {
                    eprintln!("{} {}: Flush error: {}", 
                             "❌".red(), 
                             format!("Shard {}", shard_id).bright_yellow(), 
                             e);
                    break;
                }
                bytes_written = 0;
            }
        }

        // Final flush on shutdown
        if let Err(e) = writer.flush() {
            eprintln!("{} {}: Final flush error: {}", 
                     "❌".red(), 
                     format!("Shard {}", shard_id).bright_yellow(), 
                     e);
        }

        if output_file.is_some() {
            eprintln!("{} {} writer finished", 
                     "✅".green(),
                     format!("Shard {}", shard_id).bright_yellow());
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_shard_filename() {
        // Single shard
        assert_eq!(generate_shard_filename("output", 0, 1), "output.txt");
        assert_eq!(generate_shard_filename("output.txt", 0, 1), "output.txt");
        
        // Multiple shards
        assert_eq!(generate_shard_filename("output", 0, 2), "output-0.txt");
        assert_eq!(generate_shard_filename("output", 1, 2), "output-1.txt");
        assert_eq!(generate_shard_filename("output.json", 0, 2), "output-0.json");
        assert_eq!(generate_shard_filename("output.json", 1, 2), "output-1.json");
    }

    #[test]
    fn test_round_robin_distribution() {
        let (tx1, _rx1) = bounded(10);
        let (tx2, _rx2) = bounded(10);
        let (tx3, _rx3) = bounded(10);
        
        let rr = RoundRobin::new(vec![tx1, tx2, tx3]);
        
        // Test round-robin behavior by checking sender addresses
        let first_cycle = [
            rr.next() as *const Sender<String>,
            rr.next() as *const Sender<String>,
            rr.next() as *const Sender<String>,
        ];
        
        let second_cycle = [
            rr.next() as *const Sender<String>,
            rr.next() as *const Sender<String>,
            rr.next() as *const Sender<String>,
        ];
        
        // Should cycle through the same senders in the same order
        assert_eq!(first_cycle, second_cycle);
        
        // All three should be different
        assert_ne!(first_cycle[0], first_cycle[1]);
        assert_ne!(first_cycle[1], first_cycle[2]);
        assert_ne!(first_cycle[0], first_cycle[2]);
    }
}
