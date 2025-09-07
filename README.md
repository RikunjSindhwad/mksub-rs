# mksub-rs

Ultra-fast subdomain combinator in Rust - A high-performance rewrite optimized for very large wordlists and high fan-out scenarios.

> **Note**: This is a Rust reimplementation of the original [mksub](https://github.com/trickest/mksub) tool by Trickest. The original idea and design concepts come from their Go implementation. This version focuses on performance optimization and enhanced features while maintaining CLI compatibility.

## Features

- **High Performance**: Leverages Rust's rayon for parallel processing and crossbeam channels for lock-free communication
- **Memory Efficient**: Bounded queues and streaming output prevent memory exhaustion on large datasets  
- **Flexible I/O**: Supports single domain, domain files, or stdin input with optional sharded file output
- **Colored Output**: Beautiful colored terminal output with emoji indicators and smart formatting
- **Regex Filtering**: Optional wordlist filtering with case-sensitive/insensitive matching
- **Multi-level Generation**: Generates all depths from 1 to k (not just level k)
- **Graceful Shutdown**: SIGINT/SIGTERM handling with proper buffer flushing
- **Cross-Platform**: Works on Windows, Linux, and macOS with optimal performance

## Usage

```bash
# Basic usage - generate level 1 subdomains
mksub-rs -d example.com -w wordlist.txt

# Multi-level generation with file output
mksub-rs -d example.com -w wordlist.txt -l 3 -o output.txt

# Use domain file with regex filtering
mksub-rs --domain-file domains.txt -w wordlist.txt -r "^(api|dev|cdn)$" -l 2

# High-throughput with sharded output  
cat domains.txt | mksub-rs -w large_wordlist.txt -l 2 -o results.txt --shards 4 --threads 500

# Regex filtering (case-insensitive by default)
mksub-rs -d example.com -w wordlist.txt -r "api|dev" --ci-regex

# Disable colored output for scripting
mksub-rs -d example.com -w wordlist.txt --no-color

# Silent mode with file output for maximum performance
mksub-rs -d example.com -w wordlist.txt -l 2 -o results.txt --silent
```

## Options

- `-d, --domain`: Single base domain
- `--domain-file`: File containing domains (one per line)  
- `-w, --wordlist`: Wordlist file (required)
- `-r, --regex`: Regex filter for wordlist entries
- `-l, --level`: Subdomain depth (default: 1)  
- `-t, --threads`: Concurrency level (default: 100)
- `-o, --output`: Output file (stdout if omitted)
- `--silent`: Skip stdout output (auto-disabled if no output file)
- `--shards`: Number of output file shards (default: 1)
- `--buffer-mb`: Buffer size per shard in MiB (default: 100)
- `--queue`: Channel queue size (default: 100000)
- `--max-threads`: Global thread limit (default: 100000)
- `--ci-regex`: Case-insensitive regex matching (default: true)
- `-n, --no-color`: Disable colored output for scripting/piping

## Performance

Optimized for:
- Millions of generated subdomains per minute
- Stable memory usage even with huge wordlists  
- Efficient round-robin load balancing across output shards
- Minimal allocation overhead through buffer reuse
- Smart output formatting that doesn't interfere with performance

## Visual Output

When outputting to terminal, `mksub-rs` provides:
- 🚀 Colored status messages with progress indicators
- 🎨 Syntax-highlighted subdomain output (subdomains in blue, domains in white)  
- ⏳ Progress indicators for file operations
- ✅ Completion confirmations
- 🚫 Automatic color detection (disabled for non-TTY outputs)

Use `--no-color` or `-n` to disable colors for scripting or when piping output.

## Architecture

- **main.rs**: CLI parsing and orchestration
- **io_utils.rs**: File/stdin input processing with deduplication  
- **rr.rs**: Round-robin distribution and writer thread management
- **generator.rs**: Parallel subdomain generation with iterative algorithms

Built with latest versions of:
- `clap 4.5.47` for CLI parsing
- `rayon 1.11` for parallel processing  
- `crossbeam-channel 0.5.15` for lock-free communication
- `regex 1.11.2` for wordlist filtering
- `ctrlc 3.5` for graceful shutdown
- `colored 2.2` for beautiful terminal output
- `anyhow 1.0.99` for error handling

## Building

```bash
cargo build --release
```

The optimized binary will be available at `target/release/mksub-rs.exe` (Windows) or `target/release/mksub-rs` (Unix).

### Requirements

- Rust 1.87+ with 2024 edition support
- Works on Windows, Linux, and macOS

## Testing

```bash
cargo test
```

Includes comprehensive unit and integration tests covering all major functionality including:
- Multi-level subdomain generation
- Regex filtering (case-sensitive/insensitive)
- Round-robin file sharding
- Word normalization and deduplication
- Output formatting and error handling

## License

MIT License - feel free to use in your security testing and reconnaissance workflows.

## Credits

This project is inspired by and maintains compatibility with the original [mksub](https://github.com/trickest/mksub) tool created by [Trickest](https://github.com/trickest). 

**Original Project**: https://github.com/trickest/mksub/tree/master

The Rust implementation adds performance optimizations, colored output, enhanced error handling, and additional features while preserving the core functionality and CLI interface of the original Go version.
