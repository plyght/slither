<div align='center'>
    <br/>
    <br/>
    <img src="https://github.com/user-attachments/assets/cc330a78-001a-42af-a778-ef4d69599407" alt="slither-logo" width="300"/>
    <br/>
    <br/>
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
