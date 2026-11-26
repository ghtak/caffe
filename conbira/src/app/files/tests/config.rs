mod contract {
    use espr::validate::Validate as _;

    use crate::app::files::test_support::config;

    #[test]
    fn allows_http_endpoints_on_private_ipv4_networks() {
        for endpoint in [
            "http://192.168.1.20:19000",
            "http://10.0.0.20:19000",
            "http://172.16.0.20:19000",
        ] {
            let settings = config(Some(endpoint.into()));
            // 사설 IPv4의 S3 서버는 HTTP로 연결할 수 있다.
            settings.validate().unwrap();
        }
    }

    #[test]
    fn validates_required_credentials_policies_and_local_endpoints() {
        let valid = config(Some("http://127.0.0.1:19000".into()));
        valid.validate().unwrap();
        let mut cases = Vec::new();
        let mut invalid = valid.clone();
        invalid.s3.secret_access_key.clear();
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.s3.access_key_id = " ".into();
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.s3.bucket.clear();
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.s3.key_prefix = " ".into();
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.pending_ttl_seconds = 0;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.cleanup_interval_seconds = 0;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.download_url_ttl_seconds = 604801;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.max_files_per_upload = 0;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.max_file_size_bytes = 0;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.max_file_size_bytes = i64::MAX as u64 + 1;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.s3.endpoint = Some("http://example.com".into());
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.s3.endpoint = Some("http://8.8.8.8:19000".into());
        cases.push(invalid);
        let mut invalid = valid;
        invalid.s3.endpoint = Some("https://user:secret@example.com".into());
        cases.push(invalid);
        // 잘못된 설정은 보정하지 않고 시작 설정 오류로 거절한다.
        for invalid in cases {
            assert_eq!(
                invalid.validate().unwrap_err().code,
                crate::Code::InvalidConfiguration
            );
        }
    }

    #[test]
    fn configuration_debug_hides_both_credentials() {
        let settings = config(None);
        let debug = format!("{settings:?}");
        // 상위 설정을 출력해도 인증 값은 노출되지 않는다.
        assert!(!debug.contains(&settings.s3.access_key_id));
        assert!(!debug.contains(&settings.s3.secret_access_key));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn environment_overrides_file_bucket() -> anyhow::Result<()> {
        const MARKER: &str = "CAFFE_FILES_CONFIG_CHILD";
        if std::env::var_os(MARKER).is_none() {
            let (_, module) = module_path!().split_once("::").unwrap();
            let output = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    &format!("{module}::environment_overrides_file_bucket"),
                    "--nocapture",
                ])
                .env(MARKER, "1")
                .env("CONBIRA_FILES__S3__BUCKET", "overridden-test-bucket")
                .env("CONBIRA_FILES__MAX_FILES_PER_UPLOAD", "3")
                .output()?;
            // 자식 프로세스가 덮어쓰기 검증을 실제로 수행한다.
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("verified files override"));
            return Ok(());
        }
        let settings = crate::config::Config::load(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
            Some(std::path::Path::new("/nonexistent-files-test.env")),
        )?;
        // 파일 설정보다 환경 변수 버킷이 우선한다.
        assert_eq!(settings.files.s3.bucket, "overridden-test-bucket");
        assert_eq!(settings.files.max_files_per_upload, 3);
        println!("verified files override");
        Ok(())
    }
}
