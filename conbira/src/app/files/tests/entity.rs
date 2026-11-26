mod contract {
    use espr::validate::Validate as _;

    use crate::app::files::entity::{File, FileStatus};

    fn file() -> File {
        let now = jiff::Timestamp::now();
        File {
            file_id: uuid::Uuid::new_v4(),
            group_id: uuid::Uuid::new_v4(),
            resource_type: None,
            resource_key: None,
            status: FileStatus::Pending,
            uploaded_by: 1,
            s3_bucket: "bucket".into(),
            s3_object_key: "key".into(),
            original_filename: "report.txt".into(),
            size_bytes: 0,
            content_type: "text/plain".into(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    #[test]
    fn rejects_invalid_metadata_and_state_combinations_in_application() {
        let mut invalid = Vec::new();
        let mut row = file();
        row.size_bytes = -1;
        invalid.push(row);
        let mut row = file();
        row.original_filename = " ".into();
        invalid.push(row);
        let mut row = file();
        row.content_type.clear();
        invalid.push(row);
        let mut row = file();
        row.s3_bucket.clear();
        invalid.push(row);
        let mut row = file();
        row.s3_object_key.clear();
        invalid.push(row);
        let mut row = file();
        row.resource_type = Some("post".into());
        invalid.push(row);
        let mut row = file();
        row.resource_type = Some(" ".into());
        row.resource_key = Some("1".into());
        invalid.push(row);
        let mut row = file();
        row.status = FileStatus::Attached;
        invalid.push(row);
        let mut row = file();
        row.status = FileStatus::Deleted;
        invalid.push(row);
        let mut row = file();
        row.deleted_at = Some(row.updated_at);
        invalid.push(row);
        // DB CHECK에 의존하지 않고 앱 모델이 각 위반을 거절한다.
        for row in invalid {
            assert_eq!(
                row.validate().unwrap_err().code,
                crate::Code::InvalidRequest
            );
        }
    }

    #[test]
    fn permits_bound_pending_and_preserves_binding_on_deleted_files() {
        let mut row = file();
        row.resource_type = Some("post".into());
        row.resource_key = Some("1".into());
        row.validate().unwrap();
        row.status = FileStatus::Attached;
        row.validate().unwrap();
        row.status = FileStatus::Deleted;
        row.deleted_at = Some(row.updated_at);
        // 0바이트 파일과 삭제 행의 기존 귀속은 유효하다.
        row.validate().unwrap();
        assert_eq!(row.resource_key.as_deref(), Some("1"));
    }
}
