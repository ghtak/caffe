mod contract {
    use std::time::Duration;

    use uuid::Uuid;

    use crate::app::files::entity::FileStatus;
    use crate::app::files::repository;
    use crate::app::files::test_support::TestDatabase;

    #[tokio::test]
    async fn pending_registration_is_invisible_until_commit() -> anyhow::Result<()> {
        let fixture = TestDatabase::new().await?;
        let mut writer = fixture.conn().await?;
        let mut reader = fixture.conn().await?;
        let mut tx = writer.begin().await?;
        let command = fixture.file(Uuid::new_v4());
        let file = repository::insert(&mut tx.handle(), &command).await?;
        // 호출자 커밋 전에는 다른 연결에서 보이지 않는다.
        assert!(
            repository::find_by_id(&mut reader.handle(), file.file_id)
                .await?
                .is_none()
        );
        tx.commit().await?;
        let stored = repository::find_by_id(&mut reader.handle(), file.file_id)
            .await?
            .unwrap();
        assert_eq!(file, stored);
        assert_eq!(stored.status, FileStatus::Pending);
        assert_eq!(stored.created_at, stored.updated_at);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn caller_rollback_removes_pending_metadata() -> anyhow::Result<()> {
        let fixture = TestDatabase::new().await?;
        let mut connection = fixture.conn().await?;
        let mut tx = connection.begin().await?;
        let command = fixture.file(Uuid::new_v4());
        repository::insert(&mut tx.handle(), &command).await?;
        tx.rollback().await?;
        // 저장소는 자체 커밋하지 않으므로 롤백으로 행이 사라진다.
        assert!(
            repository::find_by_id(&mut connection.handle(), command.file_id)
                .await?
                .is_none()
        );
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn concurrent_additions_to_existing_group_do_not_block_each_other() -> anyhow::Result<()>
    {
        let fixture = TestDatabase::new().await?;
        let mut first = fixture.conn().await?;
        let mut second = fixture.conn().await?;
        let group_id = Uuid::new_v4();
        let initial = fixture.file(group_id);
        let mut setup_tx = first.begin().await?;
        repository::insert(&mut setup_tx.handle(), &initial).await?;
        setup_tx.commit().await?;

        let mut first_tx = first.begin().await?;
        let mut second_tx = second.begin().await?;
        let first_file = fixture.file(group_id);
        let second_file = fixture.file(group_id);
        repository::insert(&mut first_tx.handle(), &first_file).await?;
        // 같은 그룹의 다른 파일 추가는 선행 삽입의 커밋을 기다리지 않는다.
        tokio::time::timeout(
            Duration::from_secs(5),
            repository::insert(&mut second_tx.handle(), &second_file),
        )
        .await??;
        second_tx.commit().await?;
        first_tx.commit().await?;
        let rows = repository::list_by_group_id(&mut first.handle(), group_id).await?;
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|row| row.file_id == first_file.file_id));
        assert!(rows.iter().any(|row| row.file_id == second_file.file_id));
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn foreign_key_rejects_unknown_uploader() -> anyhow::Result<()> {
        let fixture = TestDatabase::new().await?;
        let mut connection = fixture.conn().await?;
        let mut tx = connection.begin().await?;
        let mut command = fixture.file(Uuid::new_v4());
        command.uploaded_by = -1;
        // 사용자 FK는 유지되므로 없는 업로더는 저장되지 않는다.
        assert!(
            repository::insert(&mut tx.handle(), &command)
                .await
                .is_err()
        );
        tx.rollback().await?;
        assert!(
            repository::find_by_id(&mut connection.handle(), command.file_id)
                .await?
                .is_none()
        );
        fixture.cleanup().await
    }
    #[tokio::test]
    async fn primary_key_rejects_duplicate_file_id() -> anyhow::Result<()> {
        let fixture = TestDatabase::new().await?;
        let mut connection = fixture.conn().await?;
        let mut tx = connection.begin().await?;
        let command = fixture.file(Uuid::new_v4());
        repository::insert(&mut tx.handle(), &command).await?;
        let mut duplicate = fixture.file(Uuid::new_v4());
        duplicate.file_id = command.file_id;
        duplicate.s3_object_key = format!("test-files/{}", duplicate.file_id);
        // 같은 파일 ID는 별도 중복 조회 없이 PK로 거절된다.
        assert!(
            repository::insert(&mut tx.handle(), &duplicate)
                .await
                .is_err()
        );
        tx.rollback().await?;
        fixture.cleanup().await
    }
}
