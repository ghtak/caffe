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

# Run the CLI with the workspace configuration file.
run-cli:
    cargo run -p espr-cli -- --config espr-cli/config.toml
