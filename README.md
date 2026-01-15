# ohabench

An interactive menu wrapper for [oha](https://github.com/hatoo/oha) that automates load testing with breaking point detection and graph generation.

## Features

- Interactive CLI menu for configuring benchmarks
- Automatic ramping from start to max request rate
- Breaking point detection (error rate, latency, rate limiting)
- PNG graph generation with error rate and p99 latency visualization
- Business scale indicators and DAU estimates
- Text report generation

## Requirements

- [oha](https://github.com/hatoo/oha) must be installed and available in PATH

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Interactive mode
ohabench

# Direct mode
ohabench https://example.com --max-rate 1000 --step 50
```

## License

MIT
