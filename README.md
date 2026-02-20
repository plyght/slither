<div align='center'>
    <h3>Slither</h3>
    <p>A fast, hybrid web search engine with text and semantic search capabilities</p>
    <br/>
    <br/>
</div>

A high-performance hybrid search engine that combines BM25 full-text search with semantic embeddings. Slither crawls the web, extracts content, builds dual indexes (text + vector), and serves fast hybrid search queries through CLI or HTTP API.

## Features

- **Hybrid Search**: Combines BM25 text scoring with semantic embeddings for relevance
- **Concurrent Crawling**: High-throughput web crawler with configurable workers and rate limiting
- **Content Extraction**: Parses HTML to extract titles, headings, body text, links, and metadata
- **Dual Indexing**: Separate indexes for full-text (BM25) and semantic (dense vectors) search
- **ONNX Runtime**: Uses CPU-optimized ONNX runtime for embedding generation (all-MiniLM-L6-v2)
- **HTTP API**: Serve search queries via built-in HTTP server
- **Robots.txt Compliance**: Respects robots.txt by default with configurable behavior
- **Memory-Mapped Storage**: Efficient persistent storage using memory-mapped files

## Install

```bash
# From source
git clone https://github.com/plyght/slither.git
cd slither
cargo build --release
sudo cp target/release/slither /usr/local/bin/

# Or install with cargo
cargo install --path crates/slither-cli
```

## Usage

```bash
# Crawl URLs and build search index
slither crawl https://example.com --depth 2 --concurrent 20

# Search the index (hybrid mode by default)
slither search "your query here" --limit 10

# Search in specific mode
slither search "query" --mode text      # BM25 only
slither search "query" --mode semantic   # Embeddings only
slither search "query" --mode hybrid    # Combined scoring

# Start HTTP API server
slither serve --port 8080

# Show index statistics
slither stats
```

## Configuration

Create a `slither.json` file to customize behavior:

```json
{
  "crawler": {
    "max_concurrent": 50,
    "max_depth": 3,
    "rate_limit_per_second": 10,
    "user_agent": "SlitherBot/0.1",
    "respect_robots": true,
    "request_timeout_secs": 30
  },
  "index": {
    "data_dir": "slither_data/index"
  },
  "embedder": {
    "model_path": "models/all-MiniLM-L6-v2.onnx",
    "dimensions": 384,
    "data_dir": "slither_data/vectors"
  },
  "data_dir": "slither_data"
}
```

Configuration is searched in: CLI path -> `slither.json` -> defaults.

## Architecture

- `slither-cli`: CLI orchestration, command parsing, and API server
- `snake`: HTTP fetcher with rate limiting and concurrent worker pool
- `fang`: HTML content extraction (title, body, links, headings, metadata)
- `tome`: BM25 full-text search index with memory-mapped storage
- `iris`: Vector store using ONNX runtime for semantic embeddings
- `venom`: Hybrid search fusion combining text and semantic scores
- `slither-core`: Shared types, configuration, and error handling

## Production Deployment

Currently running on a Hetzner Cloud server (CX22, 4GB RAM, Ubuntu 24.04) with Cloudflare R2 for durable backup.

### Infrastructure

| Component | Details |
|-----------|---------|
| **Server** | Hetzner CX22 (2 vCPU, 4GB RAM, 40GB disk) |
| **Storage** | Local disk + Cloudflare R2 bucket (`slither`) |
| **API** | `http://<server-ip>:8080` via systemd |
| **Crawl** | Automated every 2 hours via systemd timer |
| **Backup** | `rclone sync` to R2 after each crawl cycle |

### File Layout

```
/usr/local/bin/slither              # Binary
/usr/local/bin/slither-crawl-loop   # Crawl automation script
/home/nico/slither/slither.json     # Config
/home/nico/slither/models/          # ONNX model files
/mnt/r2-slither/                    # Index + vector data directory
  index/                            # BM25 index (docs.bin, index.bin, meta.json, terms.bin)
  vectors/                          # Semantic vectors (vectors.bin, vecmap.bin)
```

### systemd Services

**`slither-serve.service`** — API server with auto-restart:
```bash
systemctl status slither-serve
```

**`slither-crawl-job.timer`** — crawl every 2 hours across 29 seed URLs in 3 batches (news, docs, research), with 30-minute timeout per batch. After each cycle: sync to R2, restart serve to pick up new data.
```bash
systemctl status slither-crawl-job.timer
journalctl -u slither-crawl-job.service -f  # watch crawl logs
```

### Setup from Scratch

```bash
# On a fresh Ubuntu server:
bash scripts/setup-hetzner.sh

# Configure rclone for R2:
rclone config  # add r2 remote with Cloudflare S3-compatible credentials

# Install systemd units and crawl script, then:
sudo systemctl enable --now slither-serve
sudo systemctl enable --now slither-crawl-job.timer
```

## Development

```bash
# Build the project
cargo build

# Run tests
cargo test

# Build release (with optimizations)
cargo build --release
```

Requires Rust 1.70+. Key dependencies: tokio, reqwest, ratatui, ort (ONNX), tokenizers, serde.

## License

MIT License
