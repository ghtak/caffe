# 워크스페이스

```Cargo.toml
[workspace]
resolver = "3"
members = []
```

# 프로젝트 추가

```shell
cargo new espr --lib
cargo new espr-cli --bin
```

# 프로젝트 의존성

```espr-cli/Cargo.toml
[dependencies]
espr = { path = "../espr" }
```

# rustfmt

포맷 설정

```rustfmt.toml
# Nightly 불안정 옵션 활성화
unstable_features = true

# Stable 옵션 
edition = "2024" # Rust 2024 에디션
max_width = 100  # 한 줄 최대 너비
tab_spaces = 4   # 들여쓰기 4칸

# Nightly 전용(Unstable) 옵션 예시

imports_granularity = "Module"     # use 문 병합 방식 (Crate, Module, Item 등)
group_imports = "StdExternalCrate" # std, 외부 크레이트, 내부 모듈 순으로 import 정렬
format_code_in_doc_comments = true # 주석(///) 내부의 코드 블록도 포맷팅
wrap_comments = true               # 주석 줄바꿈
fn_params_layout = "Tall"          # 함수 인자가 길면 블록 스타일로 줄바꿈
fn_single_line = true              # 단일 표현식 함수는 한 줄로

# skip 은 다음과 같이
# #[rustfmt::skip]
# fn skip_fn() {}
#
# #[rustfmt::skip]
# const skip_arr: [] = []
```

실행방법

```shell
cargo +nightly fmt
```

영구 적용

```rust-toolchain.toml
[toolchain]
channel = "nightly"
components = ["rustfmt"]
```

VsCode 를 통한 적용

```settings.json
{
  "rust-analyzer.rustfmt.extraArgs": [
    "+nightly"
  ]
}
```