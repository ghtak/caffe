[windows]
set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# 사용 가능한 명령 목록 표시
default:
    @just --list

# Nightly rustfmt로 워크스페이스 서식 정리
fmt:
    cargo +nightly fmt --all

# 워크스페이스 전체 테스트 실행
test-all:
    cargo test --workspace

# 워크스페이스 테스트를 실행하고 표준 출력 표시
test-show-console:
    cargo test --workspace -- --nocapture

# espr-cli 디렉터리의 설정으로 서버 실행
[working-directory: 'espr-cli']
cli-run:
    cargo run -p espr-cli -- --config config.toml

# 설정된 경로에 localhost용 TLS 인증서와 키 생성
[working-directory: 'espr-cli']
cli-generate-tls:
    cargo run -p espr-cli -- --config config.toml --generate-tls localhost 127.0.0.1
