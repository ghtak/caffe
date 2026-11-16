[windows]
set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# 사용 가능한 명령 목록 표시
default:
    @just --list

# Nightly rustfmt로 워크스페이스 서식 정리
fmt:
    cargo +nightly fmt --all

# 워크스페이스의 기본 테스트 실행 (ignored 제외)
test:
    cargo test --workspace

# 기본 테스트 실행 및 표준 출력 표시
test-show-console:
    cargo test --workspace -- --nocapture

# ignored 테스트만 실행
test-ignored:
    cargo test --workspace -- --ignored

# ignored를 포함한 모든 테스트 실행
test-all:
    cargo test --workspace -- --include-ignored

# espr-cli 디렉터리의 설정으로 서버 실행
[working-directory: 'espr-cli']
cli-run:
    cargo run -p espr-cli -- --config config.toml

# 설정된 경로에 localhost용 TLS 인증서와 키 생성
[working-directory: 'espr-cli']
cli-generate-tls:
    cargo run -p espr-cli -- --config config.toml --generate-tls localhost 127.0.0.1

# conbira 서버 실행
[working-directory: 'conbira']
conbira-run:
    cargo run -p conbira --bin conbira --

# conbira CLI 실행
[working-directory: 'conbira']
conbira-cli:
    cargo run -p conbira --bin conbira-cli --

# 설정된 경로에 localhost용 TLS 인증서와 키 생성
[working-directory: 'conbira']
conbira-generate-tls:
    cargo run -p conbira --bin conbira-cli -- --generate-tls localhost 127.0.0.1

# 설정된 경로에 Ed25519 키 쌍 생성
[working-directory: 'conbira']
conbira-generate-ed25519:
    cargo run -p conbira --bin conbira-cli -- --generate-ed25519
