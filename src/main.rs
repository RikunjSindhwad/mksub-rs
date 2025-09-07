mod io_utils;
mod rr;
mod generator;

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::io::{self, IsTerminal, Write};

#[derive(Parser)]
#[command(
    name = "mksub-rs",
    about = "Generate subdomains by prepending wordlist entries to base domains up to a specified depth. Optimized for very large wordlists and high fan-out.",
    long_about = "Generate subdomains by prepending wordlist entries to base domains up to a specified depth.\nOptimized for very large wordlists and high fan-out.\n\nDeveloped by: https://robensive.in",
    version
)]
struct Args {
    /// Single base domain (e.g., example.com)
    #[arg(short, long)]
    domain: Option<String>,

    /// File with base domains, one per line
    #[arg(long = "domain-file")]
    domain_file: Option<String>,

    /// Wordlist file (one token per line)
    #[arg(short, long)]
    wordlist: String,

    /// Optional Rust regex to filter wordlist entries (matched anywhere, case-insensitive by default)
    #[arg(short, long)]
    regex: Option<String>,

    /// Subdomain depth (k). Outputs include all depths in [1..k], matching Go behavior
    #[arg(short, long, default_value = "1")]
    level: u32,

    /// Concurrency per level (throttle)
    #[arg(short, long, default_value = "100")]
    threads: usize,

    /// Write results to file instead of stdout. If omitted, stdout is used
    #[arg(short, long)]
    output: Option<String>,

    /// Skip writing to stdout (faster). Automatically set to false when --output is omitted
    #[arg(long, default_value = "true")]
    silent: bool,

    /// Number of output shards. When > 1, write to multiple files using round-robin
    #[arg(long, default_value = "1")]
    shards: usize,

    /// Writer buffer flush threshold in MiB (per shard)
    #[arg(long = "buffer-mb", default_value = "100")]
    buffer_mb: usize,

    /// Size of each writer channel queue
    #[arg(long, default_value = "100000")]
    queue: usize,

    /// Global hard cap on worker threads
    #[arg(long = "max-threads", default_value = "100000")]
    max_threads: usize,

    /// Make regex case-insensitive by default. Disable to use exact-case
    #[arg(long = "ci-regex", default_value = "true")]
    ci_regex: bool,

    /// Disable colored output
    #[arg(long = "no-color", short = 'n')]
    no_color: bool,
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    let mut args = Args::parse();

    // Initialize colored output based on args and TTY detection
    if args.no_color || !io::stderr().is_terminal() {
        colored::control::set_override(false);
    } else {
        colored::control::set_override(true);
    }

    // Behavioral parity: If --output is omitted, force --silent=false
    if args.output.is_none() {
        args.silent = false;
    }

    // Check for required inputs
    if args.domain.is_none() && args.domain_file.is_none() && io::stdin().is_terminal() {
        eprintln!("{} Must provide either --domain, --domain-file, or pipe domains via stdin", 
                  "Error:".red().bold());
        std::process::exit(1);
    }

    // Set up graceful shutdown with double Ctrl+C handling
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown_flag.clone();
    let force_exit = Arc::new(AtomicBool::new(false));
    let force_exit_clone = force_exit.clone();
    
    ctrlc::set_handler(move || {
        if shutdown_clone.load(Ordering::Relaxed) {
            // Second Ctrl+C - force exit immediately
            eprintln!("\n{} {}", "💀".red(), "Force exit! Terminating immediately...".red().bold());
            force_exit_clone.store(true, Ordering::Relaxed);
            std::process::exit(130); // Standard exit code for Ctrl+C
        } else {
            // First Ctrl+C - graceful shutdown
            eprintln!("\n{} {} {}", 
                "⚡".yellow(), 
                "Received interrupt signal, shutting down gracefully...".yellow(),
                "(Press Ctrl+C again to force exit)".bright_black()
            );
            shutdown_clone.store(true, Ordering::Relaxed);
            SHUTDOWN.store(true, Ordering::Relaxed);
        }
    })
    .context("Error setting Ctrl-C handler")?;

    // Configure rayon thread pool
    let worker_threads = std::cmp::min(args.max_threads, rayon::current_num_threads());
    if worker_threads != rayon::current_num_threads() {
        rayon::ThreadPoolBuilder::new()
            .num_threads(worker_threads)
            .build_global()
            .context("Failed to configure rayon thread pool")?;
    }

    // Read base domains
    let bases = io_utils::read_domains(
        args.domain.as_deref(),
        args.domain_file.as_deref(),
    )?;

    if bases.is_empty() {
        eprintln!("{} No valid base domains found", "Error:".red().bold());
        std::process::exit(1);
    }

    // Read and process wordlist
    let words = io_utils::read_wordlist(
        &args.wordlist,
        args.regex.as_deref(),
        args.ci_regex,
    )?;

    if words.is_empty() {
        eprintln!("{} No valid words found in wordlist", "Error:".red().bold());
        std::process::exit(1);
    }

    // Print colorful status information
    eprintln!(
        "{} {} {} domains and {} unique words, generating up to level {}",
        "🚀".bright_blue(),
        "Loaded".bright_green().bold(),
        bases.len().to_string().bright_cyan().bold(),
        words.len().to_string().bright_cyan().bold(),
        args.level.to_string().bright_magenta().bold()
    );

    // Show attribution when not silent
    if !args.silent {
        eprintln!("{} {} {}", 
            "⚡".bright_yellow(), 
            "mksub-rs by".bright_white(),
            "https://robensive.in".bright_blue().underline()
        );
    }
    
    // Ensure status is printed before subdomain generation starts
    let _ = io::stderr().flush();

    // Initialize round-robin writers
    let (sender, writer_handles) = rr::init_writers(
        args.output.as_deref(),
        args.shards,
        args.buffer_mb,
        args.queue,
        args.silent,
        shutdown_flag.clone(),
    )?;

    // Create emission function
    let emit = |line: String| {
        if !SHUTDOWN.load(Ordering::Relaxed) && sender.send(line).is_err() {
            // Channel closed, writers shutting down
        }
    };

    // Generate subdomains
    for base in &bases {
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }

        generator::generate_subdomains(
            base,
            &words,
            args.level,
            args.threads,
            emit,
        );
    }

    // Signal completion and wait for writers
    drop(sender);
    
    // Ensure stdout is flushed before printing status to stderr
    let _ = io::stdout().flush();
    
    // Only show status messages when not writing to stdout (when silent or when output file is specified)
    if args.silent || args.output.is_some() {
        eprintln!("{} Waiting for writers to finish...", "⏳".yellow());
    }
    
    // Wait for writers with timeout and shutdown checking
    use std::time::{Duration, Instant};
    let start_time = Instant::now();
    let mut remaining_handles = writer_handles;
    
    while !remaining_handles.is_empty() {
        // Check if we should force exit
        if SHUTDOWN.load(Ordering::Relaxed) && start_time.elapsed() > Duration::from_secs(3) {
            eprintln!("{} {} {} writers after 3 seconds of graceful shutdown", 
                "⚠️".yellow(), 
                "Force terminating".yellow().bold(),
                remaining_handles.len()
            );
            break;
        }
        
        // Try to join completed handles
        remaining_handles.retain(|handle| !handle.is_finished());
        
        if remaining_handles.is_empty() {
            break;
        }
        
        // Small sleep to avoid busy waiting
        std::thread::sleep(Duration::from_millis(50));
        
        // Timeout after 10 seconds total
        if start_time.elapsed() > Duration::from_secs(10) {
            eprintln!("{} {} {} writers after 10 seconds - force terminating", 
                "⏰".red(), 
                "Timeout! Force terminating".red().bold(),
                remaining_handles.len()
            );
            break;
        }
    }
    
    // Join any remaining handles quickly
    for handle in remaining_handles {
        if let Err(e) = handle.join() {
            eprintln!("{} Writer thread error: {:?}", "❌".red(), e);
        }
    }

    if args.silent || args.output.is_some() {
        eprintln!("{} {}", "✅".green(), "Generation complete".bright_green().bold());
    }
    Ok(())
}
