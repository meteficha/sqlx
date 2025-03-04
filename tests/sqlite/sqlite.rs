use futures::TryStreamExt;
use sqlx::{
    query, sqlite::Sqlite, Connect, Connection, Executor, Row, SqliteConnection, SqlitePool,
};
use sqlx_test::new;

#[sqlx_macros::test]
async fn it_connects() -> anyhow::Result<()> {
    Ok(new::<Sqlite>().await?.ping().await?)
}

#[sqlx_macros::test]
async fn it_fetches_and_inflates_row() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    // process rows, one-at-a-time
    // this reuses the memory of the row

    {
        let expected = [15, 39, 51];
        let mut i = 0;
        let mut s = conn.fetch("SELECT 15 UNION SELECT 51 UNION SELECT 39");

        while let Some(row) = s.try_next().await? {
            let v1 = row.get::<i32, _>(0);
            assert_eq!(expected[i], v1);
            i += 1;
        }
    }

    // same query, but fetch all rows at once
    // this triggers the internal inflation

    let rows = conn
        .fetch_all("SELECT 15 UNION SELECT 51 UNION SELECT 39")
        .await?;

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<i32, _>(0), 15);
    assert_eq!(rows[1].get::<i32, _>(0), 39);
    assert_eq!(rows[2].get::<i32, _>(0), 51);

    // same query but fetch the first row a few times from a non-persistent query
    // these rows should be immediately inflated

    let row1 = conn
        .fetch_one("SELECT 15 UNION SELECT 51 UNION SELECT 39")
        .await?;

    assert_eq!(row1.get::<i32, _>(0), 15);

    let row2 = conn
        .fetch_one("SELECT 15 UNION SELECT 51 UNION SELECT 39")
        .await?;

    assert_eq!(row1.get::<i32, _>(0), 15);
    assert_eq!(row2.get::<i32, _>(0), 15);

    // same query (again) but make it persistent
    // and fetch the first row a few times

    let row1 = conn
        .fetch_one(query("SELECT 15 UNION SELECT 51 UNION SELECT 39"))
        .await?;

    assert_eq!(row1.get::<i32, _>(0), 15);

    let row2 = conn
        .fetch_one(query("SELECT 15 UNION SELECT 51 UNION SELECT 39"))
        .await?;

    assert_eq!(row1.get::<i32, _>(0), 15);
    assert_eq!(row2.get::<i32, _>(0), 15);

    Ok(())
}

#[sqlx_macros::test]
async fn it_fetches_in_loop() -> anyhow::Result<()> {
    // this is trying to check for any data races
    // there were a few that triggered *sometimes* while building out StatementWorker
    for _ in 0..1000_usize {
        let mut conn = new::<Sqlite>().await?;
        let v: Vec<(i32,)> = sqlx::query_as("SELECT 1").fetch_all(&mut conn).await?;

        assert_eq!(v[0].0, 1);
    }

    Ok(())
}

#[sqlx_macros::test]
async fn it_executes_with_pool() -> anyhow::Result<()> {
    let pool: SqlitePool = SqlitePool::builder()
        .min_size(2)
        .max_size(2)
        .test_on_acquire(false)
        .build(&dotenv::var("DATABASE_URL")?)
        .await?;

    let rows = pool.fetch_all("SELECT 1; SElECT 2").await?;

    assert_eq!(rows.len(), 2);

    Ok(())
}

#[sqlx_macros::test]
async fn it_opens_in_memory() -> anyhow::Result<()> {
    // If the filename is ":memory:", then a private, temporary in-memory database
    // is created for the connection.
    let _ = SqliteConnection::connect(":memory:").await?;

    Ok(())
}

#[sqlx_macros::test]
async fn it_opens_temp_on_disk() -> anyhow::Result<()> {
    // If the filename is an empty string, then a private, temporary on-disk database will
    // be created.
    let _ = SqliteConnection::connect("").await?;

    Ok(())
}

#[sqlx_macros::test]
async fn it_fails_to_parse() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;
    let res = conn.execute("SEELCT 1").await;

    assert!(res.is_err());

    let err = res.unwrap_err().to_string();

    assert_eq!(
        "error returned from database: near \"SEELCT\": syntax error",
        err
    );

    Ok(())
}

#[sqlx_macros::test]
async fn it_handles_empty_queries() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;
    let affected = conn.execute("").await?;

    assert_eq!(affected, 0);

    Ok(())
}

#[sqlx_macros::test]
fn it_binds_parameters() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    let v: i32 = sqlx::query_scalar("SELECT ?")
        .bind(10_i32)
        .fetch_one(&mut conn)
        .await?;

    assert_eq!(v, 10);

    let v: (i32, i32) = sqlx::query_as("SELECT ?1, ?")
        .bind(10_i32)
        .fetch_one(&mut conn)
        .await?;

    assert_eq!(v.0, 10);
    assert_eq!(v.1, 10);

    Ok(())
}

#[sqlx_macros::test]
async fn it_executes_queries() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    let _ = conn
        .execute(
            r#"
CREATE TEMPORARY TABLE users (id INTEGER PRIMARY KEY)
            "#,
        )
        .await?;

    for index in 1..=10_i32 {
        let cnt = sqlx::query("INSERT INTO users (id) VALUES (?)")
            .bind(index * 2)
            .execute(&mut conn)
            .await?;

        assert_eq!(cnt, 1);
    }

    let sum: i32 = sqlx::query_as("SELECT id FROM users")
        .fetch(&mut conn)
        .try_fold(0_i32, |acc, (x,): (i32,)| async move { Ok(acc + x) })
        .await?;

    assert_eq!(sum, 110);

    Ok(())
}

#[sqlx_macros::test]
async fn it_can_execute_multiple_statements() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    let affected = conn
        .execute(
            r#"
CREATE TEMPORARY TABLE users (id INTEGER PRIMARY KEY, other INTEGER);
INSERT INTO users DEFAULT VALUES;
            "#,
        )
        .await?;

    assert_eq!(affected, 1);

    for index in 2..5_i32 {
        let (id, other): (i32, i32) = sqlx::query_as(
            r#"
INSERT INTO users (other) VALUES (?);
SELECT id, other FROM users WHERE id = last_insert_rowid();
            "#,
        )
        .bind(index)
        .fetch_one(&mut conn)
        .await?;

        assert_eq!(id, index);
        assert_eq!(other, index);
    }

    Ok(())
}

#[sqlx_macros::test]
async fn it_interleaves_reads_and_writes() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    let mut cursor = conn.fetch(
        "
CREATE TABLE IF NOT EXISTS _sqlx_test (
    id INT PRIMARY KEY,
    text TEXT NOT NULL
);

SELECT 'Hello World' as _1;

INSERT INTO _sqlx_test (text) VALUES ('this is a test');

SELECT id, text FROM _sqlx_test;
    ",
    );

    let row = cursor.try_next().await?.unwrap();

    assert!("Hello World" == row.try_get::<&str, _>("_1")?);

    let row = cursor.try_next().await?.unwrap();

    let id: i64 = row.try_get("id")?;
    let text: &str = row.try_get("text")?;

    assert_eq!(0, id);
    assert_eq!("this is a test", text);

    Ok(())
}

#[sqlx_macros::test]
async fn it_caches_statements() -> anyhow::Result<()> {
    let mut conn = new::<Sqlite>().await?;

    for i in 0..2 {
        let row = sqlx::query("SELECT ? AS val")
            .bind(i)
            .fetch_one(&mut conn)
            .await?;

        let val: i32 = row.get("val");

        assert_eq!(i, val);
    }

    assert_eq!(1, conn.cached_statements_size());
    conn.clear_cached_statements().await?;
    assert_eq!(0, conn.cached_statements_size());

    Ok(())
}
