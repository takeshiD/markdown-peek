# fastgrep

A tiny, fast grep replacement written in Rust.

## Features

- Parallel directory walking
- Respects `.gitignore`
- Unicode aware

## Installation

```sh
git clone https://example.com/fastgrep.git
cd fastgrep
cargo install --path .
```

## Usage

```sh
fastgrep "pattern" src/
```

## Configuration

```toml
[search]
hidden = false
max_depth = 12
follow_links = false
threads = 8
ignore_case = "smart"
colors = true
context = 2
```

## License

MIT
