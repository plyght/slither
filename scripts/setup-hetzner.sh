#!/bin/bash
# Slither Setup Script for Hetzner

set -e

echo "=== Slither Search Engine Setup ==="

# Install Rust if not present
if ! command -v cargo &>/dev/null; then
	echo "Installing Rust..."
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	source "$HOME/.cargo/env"
fi

# Install build dependencies
echo "Installing build dependencies..."
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev

# Create models directory and download model
echo "Downloading ONNX model..."
mkdir -p models
cd models
curl -L -o all-MiniLM-L6-v2.onnx "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/all-MiniLM-L6-v2.onnx" ||
	curl -L -o all-MiniLM-L6-v2.onnx "https://cdn.jsdelivr.net/npm/@xenova/transformers@2.17.2/models/all-MiniLM-L6-v2/onnx/model_quantized.onnx"

curl -L -o tokenizer.json "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json" ||
	curl -L -o tokenizer.json "https://cdn.jsdelivr.net/npm/@xenova/transformers@2.17.2/models/all-MiniLM-L6-v2/tokenizer.json"

cd ..

# Build
echo "Building Slither..."
cargo build --release

echo "=== Setup complete! ==="
echo ""
echo "To start crawling:"
echo "  ./target/release/slither crawl \"https://wikipedia.org\" --depth 2 --concurrent 20 --output-dir ./index"
echo ""
echo "To start API server:"
echo "  ./target/release/slither serve --port 8080 --host 0.0.0.0 --output-dir ./index"
echo ""
echo "API endpoints:"
echo "  http://YOUR_SERVER_IP:8080/search?q=rust"
echo "  http://YOUR_SERVER_IP:8080/stats"
echo "  http://YOUR_SERVER_IP:8080/health"
