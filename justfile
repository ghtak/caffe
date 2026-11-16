[windows]
set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# 사용 가능한 명령 목록 표시
default:
    @just --list

# rustfmt
fmt:
    cargo +nightly fmt 

# all test
test-all:
    cargo test --workspace

# Run the CLI from its project directory.
[working-directory: 'espr-cli']
cli-run:
    cargo run -p espr-cli -- --config config.toml

# Generate TLS certificates for the CLI.
[working-directory: 'espr-cli']
cli-generate-tls:
    cargo run -p espr-cli -- --config config.toml --generate-tls localhost 127.0.0.1
