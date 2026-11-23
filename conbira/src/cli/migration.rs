//! SQL과 코드 마이그레이션의 실행 계약.
//!
//! 순서는 사용자가 SQL 파일명(`0001_name.sql`)과 코드 객체에 직접 지정한다.
//! 모든 작업은 재실행해도 안전하도록 각각 완결되어야 한다.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use conbira::Error;
use conbira::app::DatabaseExt as _;
use conbira::config::Config;
use conbira::error::ResultExt as _;
use espr::sqlxdb::Database;
use sqlx::Postgres;

pub(crate) mod sample_0002;

/// SQL 파일과 코드 객체가 공유하는 실행 순서.
pub(crate) type Order = u64;

/// 하나의 코드 마이그레이션.
///
/// 구현체는 동일한 작업을 반복 실행해도 안전해야 한다.
pub(crate) trait Migration {
    fn order(&self) -> Order;

    fn name(&self) -> &'static str { std::any::type_name::<Self>() }

    fn run<'a>(
        &'a self,
        database: &'a Database<Postgres>,
    ) -> Pin<Box<dyn Future<Output = conbira::Result<()>> + Send + 'a>>;
}

/// 지정한 디렉터리의 SQL 파일 하나.
pub(crate) struct SqlMigration {
    pub order: Order,
    pub path: PathBuf,
}

/// SQL과 코드 작업을 함께 정렬하고 실행하기 위한 항목.
pub(crate) enum MigrationTask {
    Sql(SqlMigration),
    Code(Box<dyn Migration>),
}

impl MigrationTask {
    pub(crate) fn order(&self) -> Order {
        match self {
            Self::Sql(migration) => migration.order,
            Self::Code(migration) => migration.order(),
        }
    }
}

/// 디렉터리의 SQL과 전달받은 코드 작업을 공통 목록으로 조합한다.
fn collect(
    directories: &[impl AsRef<Path>],
    migrations: Vec<Box<dyn Migration>>,
) -> conbira::Result<Vec<MigrationTask>> {
    let mut tasks: Vec<_> = migrations.into_iter().map(MigrationTask::Code).collect();
    for directory in directories {
        let directory = directory.as_ref();
        let entries = std::fs::read_dir(directory).map_err_internal(
            format!("Failed to read migration directory {}", directory.display()),
            conbira::here!(),
        )?;
        for entry in entries {
            let entry = entry.map_err_internal(
                format!("Failed to read entry in {}", directory.display()),
                conbira::here!(),
            )?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "sql") {
                continue;
            }
            if !entry
                .file_type()
                .map_err_internal(
                    format!("Failed to inspect {}", path.display()),
                    conbira::here!(),
                )?
                .is_file()
            {
                return Err(Error::internal(format!(
                    "Migration is not a regular file: {}",
                    path.display()
                ))
                .at(conbira::here!()));
            }
            let order = parse_order(&path)?;
            tasks.push(MigrationTask::Sql(SqlMigration { order, path }));
        }
    }
    tasks.sort_by_key(MigrationTask::order);
    for pair in tasks.windows(2) {
        if pair[0].order() == pair[1].order() {
            return Err(
                Error::internal(format!("Duplicate migration order: {}", pair[0].order()))
                    .at(conbira::here!()),
            );
        }
    }
    Ok(tasks)
}

fn parse_order(path: &Path) -> conbira::Result<Order> {
    let order = path
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('_'))
        .and_then(|(number, name)| {
            if number.len() < 4
                || !number.bytes().all(|byte| byte.is_ascii_digit())
                || name.is_empty()
            {
                return None;
            }
            number.parse().ok()
        });
    order.ok_or_else(|| {
        Error::internal(format!(
            "Invalid migration filename: {} (expected 0001_name.sql)",
            path.display()
        ))
        .at(conbira::here!())
    })
}

/// 여러 디렉터리의 SQL과 코드 객체를 하나의 순서로 실행한다.
///
/// SQL 파일명은 `0001_name.sql` 형식이며 순서는 사용자가 관리한다.
/// `config.database`로 데이터베이스를 생성해 SQL과 전달받은 코드 작업에
/// 사용한다. 각 디렉터리의 바로 아래 SQL 파일을 대상으로 하며 순서 중복은
/// 오류다. 각 작업은 재실행 및 부분 실패에 안전하게 작성해야 한다.
pub(crate) async fn run(
    config: &Config,
    directories: &[impl AsRef<Path>],
    migrations: Vec<Box<dyn Migration>>,
) -> conbira::Result<()> {
    let tasks = collect(directories, migrations)?;
    let database = Database::<Postgres>::new(&config.database)
        .await
        .map_err_internal("Failed to initialize migration database", conbira::here!())?;
    let total = tasks.len();
    println!("Running {total} migration(s)");
    for task in tasks {
        let order = task.order();
        let description = match &task {
            MigrationTask::Sql(migration) => format!("SQL {}", migration.path.display()),
            MigrationTask::Code(migration) => format!("code {}", migration.name()),
        };
        println!("[{order:04}] Starting {description}");
        let result: conbira::Result<()> = async {
            match task {
                MigrationTask::Sql(migration) => {
                    let sql = std::fs::read_to_string(&migration.path).map_err_internal(
                        format!("Failed to read migration {}", migration.path.display()),
                        conbira::here!(),
                    )?;
                    let mut connection = database.acquire_conn(conbira::here!()).await?;
                    // The SQL file is the migration itself, not interpolated
                    // user data.
                    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                        .execute(connection.handle().conn())
                        .await
                        .map_err_internal(
                            format!("Failed to execute migration {}", migration.path.display()),
                            conbira::here!(),
                        )?;
                }
                MigrationTask::Code(migration) => migration.run(&database).await?,
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            eprintln!("[{order:04}] Failed {description}");
            return Err(error);
        }
        println!("[{order:04}] Completed {description}");
    }
    println!("Completed {total} migration(s)");
    Ok(())
}

#[cfg(test)]
mod tests {
    mod contract {
        use super::super::*;

        struct Code(Order);

        impl Migration for Code {
            fn order(&self) -> Order { self.0 }
            fn run<'a>(
                &'a self,
                _: &'a Database<Postgres>,
            ) -> Pin<Box<dyn Future<Output = conbira::Result<()>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        #[test]
        fn combines_directories_and_code_and_rejects_duplicate_order() {
            let root = std::env::temp_dir().join(format!(
                "conbira-migration-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let first = root.join("first");
            let second = root.join("second");
            std::fs::create_dir_all(&first).unwrap();
            std::fs::create_dir_all(&second).unwrap();
            std::fs::write(first.join("0003_last.sql"), "").unwrap();
            std::fs::write(second.join("0001_first.sql"), "").unwrap();
            std::fs::write(second.join("notes.txt"), "").unwrap();
            let dirs = [&first, &second];
            let tasks = collect(&dirs, vec![Box::new(Code(2))]).unwrap();
            assert_eq!(
                tasks.iter().map(MigrationTask::order).collect::<Vec<_>>(),
                vec![1, 2, 3]
            );
            assert!(
                collect(&dirs, vec![Box::new(Code(1))])
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("Duplicate migration order")
            );
            std::fs::write(first.join("invalid.sql"), "").unwrap();
            assert!(
                collect(&dirs, vec![])
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("Invalid migration filename")
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
