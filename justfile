[windows]
set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# 사용 가능한 명령 목록 표시
default:
    @just --list

# Nightly rustfmt로 워크스페이스 서식 정리
[group: 'workspace']
fmt:
    cargo +nightly fmt --all

# 워크스페이스 서식 및 clippy 검사
[group: 'workspace']
check:
    cargo +nightly fmt --all -- --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

# 워크스페이스의 기본 테스트 실행 (ignored 제외)
[group: 'workspace']
test:
    cargo test --workspace --locked

# 기본 테스트 실행 및 표준 출력 표시
[group: 'workspace']
test-show-console:
    cargo test --workspace -- --nocapture

# ignored 테스트만 실행
[group: 'workspace']
test-ignored:
    cargo test --workspace -- --ignored

# ignored를 포함한 모든 테스트 실행
[group: 'workspace']
test-all:
    cargo test --workspace -- --include-ignored

# espr-cli 디렉터리의 설정으로 서버 실행
[group: 'espr-cli']
[working-directory: 'espr-cli']
espr-cli-app:
    cargo run -p espr-cli

# 설정된 경로에 localhost용 TLS 인증서와 키 생성
[group: 'espr-cli']
[working-directory: 'espr-cli']
espr-cli-generate-tls:
    cargo run -p espr-cli -- --generate-tls localhost 127.0.0.1

# conbira 서버 실행
[group: 'conbira']
[working-directory: 'conbira']
conbira-app:
    cargo run -p conbira --bin conbira

# conbira CLI 예제 명령 표시
[group: 'conbira']
[working-directory: 'conbira']
conbira-cli:
    cargo run -p conbira --bin conbira-cli

# 기본 migrations 디렉터리의 마이그레이션 실행
[group: 'conbira']
[working-directory: 'conbira']
conbira-cli-migration:
    cargo run -p conbira --bin conbira-cli -- migration migrations

# 로컬 사용자 추가 (역할은 쉼표로 구분, 기본값 user)
[group: 'conbira']
[working-directory: 'conbira']
[positional-arguments]
conbira-cli-user-create-local nickname email password role='user':
    @cargo run -p conbira --bin conbira-cli -- user-create-local --nickname "$1" --email "$2" --password "$3" --role "$4"

# 설정된 경로에 localhost용 TLS 인증서와 키 생성
[group: 'conbira']
[working-directory: 'conbira']
conbira-cli-generate-tls:
    cargo run -p conbira --bin conbira-cli -- --generate-tls localhost 127.0.0.1

# 설정된 경로에 Ed25519 키 쌍 생성
[group: 'conbira']
[working-directory: 'conbira']
conbira-cli-generate-ed25519:
    cargo run -p conbira --bin conbira-cli -- --generate-ed25519

# 프런트엔드 의존성 설치
[group: 'conbira-ui']
[working-directory: 'conbira-ui']
conbira-ui-install:
    npm ci

# 프런트엔드 개발 서버 실행
[group: 'conbira-ui']
[working-directory: 'conbira-ui']
conbira-ui-dev:
    npm run dev

# 프런트엔드 타입 검사
[group: 'conbira-ui']
[working-directory: 'conbira-ui']
conbira-ui-check:
    npm run typecheck

# 프런트엔드 프로덕션 빌드
[group: 'conbira-ui']
[working-directory: 'conbira-ui']
conbira-ui-build:
    npm run build

# 프런트엔드 프로덕션 빌드 미리보기
[group: 'conbira-ui']
[working-directory: 'conbira-ui']
conbira-ui-preview:
    npm run preview
