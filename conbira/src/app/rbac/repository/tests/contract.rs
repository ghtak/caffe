use super::super::{permission, role, role_permission, user_role};
use crate::app::container::db::{Database, Handle};

async fn setup(handle: &mut Handle<'_>) -> anyhow::Result<()> {
    for schema in [
        include_str!("../../../../../migrations/0010_user.sql"),
        include_str!("../../../../../migrations/0012_rbac.sql"),
    ] {
        let schema = schema.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
            .execute(handle.conn())
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn roles_and_permissions_support_crud_and_unique_codes() -> anyhow::Result<()> {
    let database = Database::new(&crate::test_support::database_config(1)).await?;
    let mut connection = database.conn().await?;
    let mut handle = connection.handle();
    setup(&mut handle).await?;

    let new_role = role::NewRole {
        code: "editor".into(),
        name: "Editor".into(),
        description: "Edits".into(),
    };
    let new_permission = permission::NewPermission {
        code: "post.edit".into(),
        name: "Edit post".into(),
        description: "Edit posts".into(),
    };
    let mut editor = role::insert(&mut handle, &new_role).await?;
    let mut edit = permission::insert(&mut handle, &new_permission).await?;

    // 생성한 역할과 권한은 ID와 코드로 조회되며 중복 코드는 거부된다.
    assert_eq!(
        role::find_by_id(&mut handle, editor.id).await?,
        Some(role::find_by_code(&mut handle, "editor").await?.unwrap())
    );
    assert_eq!(
        permission::find_by_id(&mut handle, edit.id).await?,
        Some(
            permission::find_by_code(&mut handle, "post.edit")
                .await?
                .unwrap()
        )
    );
    assert!(role::insert(&mut handle, &new_role).await.is_err());
    assert!(
        permission::insert(&mut handle, &new_permission)
            .await
            .is_err()
    );
    assert!(
        role::list(&mut handle)
            .await?
            .iter()
            .any(|r| r.id == editor.id)
    );
    assert!(
        permission::list(&mut handle)
            .await?
            .iter()
            .any(|p| p.id == edit.id)
    );

    editor.name = "Publisher".into();
    editor.description = "Publishes".into();
    edit.code = "post.publish".into();
    // 변경된 값이 저장되며 없는 대상의 수정은 None을 반환한다.
    assert_eq!(role::update(&mut handle, &editor).await?, Some(editor));
    assert_eq!(permission::update(&mut handle, &edit).await?, Some(edit));
    assert!(role::find_by_code(&mut handle, "missing").await?.is_none());
    assert!(permission::find_by_id(&mut handle, -1).await?.is_none());
    let mut missing_role = role::find_by_code(&mut handle, "editor").await?.unwrap();
    missing_role.id = -1;
    assert!(role::update(&mut handle, &missing_role).await?.is_none());
    let mut missing_permission = permission::find_by_code(&mut handle, "post.publish")
        .await?
        .unwrap();
    missing_permission.id = -1;
    assert!(
        permission::update(&mut handle, &missing_permission)
            .await?
            .is_none()
    );

    let editor_id = role::find_by_code(&mut handle, "editor").await?.unwrap().id;
    let edit_id = permission::find_by_code(&mut handle, "post.publish")
        .await?
        .unwrap()
        .id;
    // 연결되지 않은 항목은 삭제되고 반복 삭제는 false를 반환한다.
    assert!(role::delete(&mut handle, editor_id).await?);
    assert!(!role::delete(&mut handle, editor_id).await?);
    assert!(permission::delete(&mut handle, edit_id).await?);
    assert!(!permission::delete(&mut handle, edit_id).await?);
    Ok(())
}

#[tokio::test]
async fn user_permissions_combine_roles_without_duplicates_and_reflect_removed_links()
-> anyhow::Result<()> {
    let database = Database::new(&crate::test_support::database_config(1)).await?;
    let mut connection = database.conn().await?;
    let mut handle = connection.handle();
    setup(&mut handle).await?;

    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO conbira_auth_users (nickname) VALUES ('reader') RETURNING id",
    )
    .fetch_one(handle.conn())
    .await?;
    let admin = role::find_by_code(&mut handle, "admin").await?.unwrap();
    let user = role::find_by_code(&mut handle, "user").await?.unwrap();
    let read = permission::find_by_code(&mut handle, "rbac.read")
        .await?
        .unwrap();
    user_role::insert(&mut handle, user_id, admin.id).await?;
    user_role::insert(&mut handle, user_id, user.id).await?;

    // 두 역할에 공통인 조회 권한은 한 번만 반환되고 나머지 관리자 권한도
    // 포함된다.
    let permissions = permission::find_by_user_id(&mut handle, user_id).await?;
    assert_eq!(permissions.len(), 6);
    assert_eq!(permissions.iter().filter(|p| p.id == read.id).count(), 1);
    assert_eq!(
        user_role::list_by_user_id(&mut handle, user_id)
            .await?
            .len(),
        2
    );
    assert_eq!(
        role_permission::list_by_role_id(&mut handle, user.id).await?,
        vec![read]
    );
    assert!(
        permission::find_by_user_id(&mut handle, -1)
            .await?
            .is_empty()
    );

    // 중복 연결과 참조 중인 역할 삭제는 거부된다.
    assert!(
        user_role::insert(&mut handle, user_id, user.id)
            .await
            .is_err()
    );
    assert!(role::delete(&mut handle, user.id).await.is_err());
    assert!(
        role_permission::insert(&mut handle, user.id, -1)
            .await
            .is_err()
    );

    assert!(user_role::delete(&mut handle, user_id, admin.id).await?);
    // 관리자 역할을 회수하면 일반 사용자에게 부여된 조회 권한만 남는다.
    assert_eq!(
        permission::find_by_user_id(&mut handle, user_id)
            .await?
            .iter()
            .map(|p| p.code.as_str())
            .collect::<Vec<_>>(),
        vec!["rbac.read"]
    );
    let read_id = permission::find_by_code(&mut handle, "rbac.read")
        .await?
        .unwrap()
        .id;
    assert!(permission::delete(&mut handle, read_id).await.is_err());
    assert!(role_permission::delete(&mut handle, user.id, read_id).await?);
    assert!(!role_permission::delete(&mut handle, user.id, read_id).await?);
    assert!(
        permission::find_by_user_id(&mut handle, user_id)
            .await?
            .is_empty()
    );
    role_permission::insert(&mut handle, user.id, read_id).await?;
    // 권한을 다시 연결하면 사용자 권한 조회에 반영되고 중복 연결은 거부된다.
    assert_eq!(
        permission::find_by_user_id(&mut handle, user_id)
            .await?
            .len(),
        1
    );
    assert!(
        role_permission::insert(&mut handle, user.id, read_id)
            .await
            .is_err()
    );
    assert!(user_role::delete(&mut handle, user_id, user.id).await?);
    assert!(!user_role::delete(&mut handle, user_id, user.id).await?);
    Ok(())
}
