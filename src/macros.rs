/// Statically checked SQL query with `println!()` style syntax.
///
/// This expands to an instance of [QueryAs][crate::QueryAs] that outputs an ad-hoc anonymous struct type,
/// if the query has output columns, or `()` (unit) otherwise:
///
/// ```rust,ignore
/// # use sqlx::Connect;
/// # #[cfg(all(feature = "mysql", feature = "runtime-async-std"))]
/// # #[async_std::main]
/// # async fn main() -> sqlx::Result<()>{
/// # let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
/// #
/// # if !(db_url.starts_with("mysql") || db_url.starts_with("mariadb")) { return Ok(()) }
/// # let mut conn = sqlx::MySqlConnection::connect(db_url).await?;
/// // let mut conn = <impl sqlx::Executor>;
/// let account = sqlx::query!("select (1) as id, 'Herp Derpinson' as name")
///     .fetch_one(&mut conn)
///     .await?;
///
/// // anonymous struct has `#[derive(Debug)]` for convenience
/// println!("{:?}", account);
/// println!("{}: {}", account.id, account.name);
///
/// # Ok(())
/// # }
/// #
/// # #[cfg(any(not(feature = "mysql"), not(feature = "runtime-async-std")))]
/// # fn main() {}
/// ```
///
/// ## Requirements
/// * The `DATABASE_URL` environment variable must be set at build-time to point to a database
/// server with the schema that the query string will be checked against. All variants of `query!()`
/// use [dotenv] so this can be in a `.env` file instead.
///
///     * Or, `sqlx-data.json` must exist at the workspace root. See [Offline Mode](#offline-mode)
///       below.
///
/// * The query must be a string literal or else it cannot be introspected (and thus cannot
/// be dynamic or the result of another macro).
///
/// * The `QueryAs` instance will be bound to the same database type as `query!()` was compiled
/// against (e.g. you cannot build against a Postgres database and then run the query against
/// a MySQL database).
///
///     * The schema of the database URL (e.g. `postgres://` or `mysql://`) will be used to
///       determine the database type.
///
/// [dotenv]: https://crates.io/crates/dotenv
/// ## Query Arguments
/// Like `println!()` and the other formatting macros, you can add bind parameters to your SQL
/// and this macro will typecheck passed arguments and error on missing ones:
///
/// ```rust,ignore
/// # use sqlx::Connect;
/// # #[cfg(all(feature = "mysql", feature = "runtime-async-std"))]
/// # #[async_std::main]
/// # async fn main() -> sqlx::Result<()>{
/// # let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
/// #
/// # if !(db_url.starts_with("mysql") || db_url.starts_with("mariadb")) { return Ok(()) }
/// # let mut conn = sqlx::mysql::MySqlConnection::connect(db_url).await?;
/// // let mut conn = <impl sqlx::Executor>;
/// let account = sqlx::query!(
///         // just pretend "accounts" is a real table
///         "select * from (select (1) as id, 'Herp Derpinson' as name) accounts where id = ?",
///         1i32
///     )
///     .fetch_one(&mut conn)
///     .await?;
///
/// println!("{:?}", account);
/// println!("{}: {}", account.id, account.name);
/// # Ok(())
/// # }
/// #
/// # #[cfg(any(not(feature = "mysql"), not(feature = "runtime-async-std")))]
/// # fn main() {}
/// ```
///
/// Bind parameters in the SQL string are specific to the database backend:
///
/// * Postgres: `$N` where `N` is the 1-based positional argument index
/// * MySQL: `?` which matches arguments in order that it appears in the query
///
/// ## Nullability: Bind Parameters
/// For a given expected type `T`, both `T` and `Option<T>` are allowed (as well as either
/// behind references). `Option::None` will be bound as `NULL`, so if binding a type behind `Option`
/// be sure your query can support it.
///
/// Note, however, if binding in a `where` clause, that equality comparisons with `NULL` may not
/// work as expected; instead you must use `IS NOT NULL` or `IS NULL` to check if a column is not
/// null or is null, respectively. Note that `IS [NOT] NULL` cannot be bound as a parameter either;
/// you must modify your query string instead.
///
/// ## Nullability: Output Columns
/// In most cases, the database engine can tell us whether or not a column may be `NULL`, and
/// the `query!()` macro adjusts the field types of the returned struct accordingly.
///
/// For Postgres and SQLite, this only works for columns which come directly from actual tables,
/// as the implementation will need to query the table metadata to find if a given column
/// has a `NOT NULL` constraint. Columns that do not have a `NOT NULL` constraint or are the result
/// of an expression are assumed to be nullable and so `Option<T>` is used instead of `T`.
///
/// For MySQL, the implementation looks at [the `NOT_NULL` flag](https://dev.mysql.com/doc/dev/mysql-server/8.0.12/group__group__cs__column__definition__flags.html#ga50377f5ca5b3e92f3931a81fe7b44043)
/// of [the `ColumnDefinition` structure in `COM_QUERY_OK`](https://dev.mysql.com/doc/internals/en/com-query-response.html#column-definition):
/// if it is set, `T` is used; if it is not set, `Option<T>` is used.
///
/// MySQL appears to be capable of determining the nullability of a result column even if it
/// is the result of an expression, depending on if the expression may in any case result in
/// `NULL` which then depends on the semantics of what functions are used. Consult the MySQL
/// manual for the functions you are using to find the cases in which they return `NULL`.
///
/// To override the nullability of an output column, use [query_as!], or see below.
///
/// ## Type Overrides: Bind Parameters (Postgres only)
/// For typechecking of bind parameters, casts using `as` are treated as overrides for the inferred
/// types of bind parameters and no typechecking is emitted:
///
/// ```rust,ignore
/// #[derive(sqlx::Type)]
/// #[sqlx(transparent)]
/// struct MyInt4(i32);
///
/// let my_int = MyInt4(1);
///
/// sqlx::query!("select $1::int4 as id", my_int as MyInt4)
/// ```
///
/// In Rust 1.45 we can eliminate this redundancy by allowing casts using `as _` or type ascription
/// syntax, i.e. `my_int: _` (which is unstable but can be stripped), but this requires modifying
/// the expression which is not possible as the macros are currently implemented. Casts to `_` are
/// forbidden for now as they produce rather nasty type errors.
///
/// ## Type Overrides: Output Columns
/// Type overrides are also available for output columns, utilizing the SQL standard's support
/// for arbitrary text in column names:
///
/// * selecting a column `foo as "foo!"` (Postgres / SQLite) or `` foo as `foo!` `` overrides
/// inferred nullability and forces the column to be treated as `NOT NULL`; this is useful e.g. for
/// selecting expressions in Postgres where we cannot infer nullability:
///
/// ```rust,ignore
/// # async fn main() {
/// # let mut conn = panic!();
/// // Postgres: using a raw query string lets us use unescaped double-quotes
/// // Note that this query wouldn't work in SQLite as we still don't know the exact type of `id`
/// let record = sqlx::query!(r#"select 1 as "id!""#) // MySQL: use "select 1 as `id!`" instead
///     .fetch_one(&mut conn)
///     .await?;
///
/// // For Postgres this would have been inferred to be Option<i32> instead
/// assert_eq!(record.id, 1i32);
/// # }
///
/// ```
/// * selecting a column `foo as "foo?"` (Postgres / SQLite) or `` foo as `foo?` `` overrides
/// inferred nullability and forces the column to be treated as nullable; this is provided mainly
/// for symmetry with `!`, but also because nullability inference currently has some holes and false
/// negatives that may not be completely fixable without doing our own complex analysis on the given
/// query.
///
/// ```rust,ignore
/// # async fn main() {
/// # let mut conn = panic!();
/// // Postgres:
/// // Note that this query wouldn't work in SQLite as we still don't know the exact type of `id`
/// let record = sqlx::query!(r#"select 1 as "id?""#) // MySQL: use "select 1 as `id?`" instead
///     .fetch_one(&mut conn)
///     .await?;
///
/// // For Postgres this would have been inferred to be Option<i32> anyway
/// // but this is just a basic example
/// assert_eq!(record.id, Some(1i32));
/// # }
/// ```
///
/// One current such hole is exposed by left-joins involving `NOT NULL` columns in Postgres and
/// SQLite; as we only know nullability for a given column based on the `NOT NULL` constraint
/// of its original column in a table, if that column is then brought in via a `LEFT JOIN`
/// we have no good way to know and so continue assuming it may not be null which may result
/// in some `UnexpectedNull` errors at runtime.
///
/// Using `?` as an override we can fix this for columns we know to be nullable in practice:
///
/// ```rust,ignore
/// # async fn main() {
/// # let mut conn = panic!();
/// // Ironically this is the exact column we look at to determine nullability in Postgres
/// let record = sqlx::query!(
///     r#"select attnotnull as "attnotnull?" from (values (1)) ids left join pg_attribute on false"#
/// )
/// .fetch_one(&mut conn)
/// .await?;
///
/// // For Postgres this would have been inferred to be `bool` and we would have gotten an error
/// assert_eq!(record.attnotnull, None);
/// # }
/// ```
///
/// See [launchbadge/sqlx#367](https://github.com/launchbadge/sqlx/issues/367) for more details on this issue.
///
/// * selecting a column `foo as "foo: T"` (Postgres / SQLite) or `` foo as `foo: T` `` (MySQL)
/// overrides the inferred type which is useful when selecting user-defined custom types
/// (dynamic type checking is still done so if the types are incompatible this will be an error
/// at runtime instead of compile-time):
///
/// ```rust,ignore
/// # async fn main() {
/// # let mut conn = panic!();
/// #[derive(sqlx::Type)]
/// #[sqlx(transparent)
/// struct MyInt4(i32);
///
/// let my_int = MyInt4(1);
///
/// // Postgres/SQLite
/// sqlx::query!(r#"select 1 as "id: MyInt4""#) // MySQL: use "select 1 as `id: MyInt4`" instead
///     .fetch_one(&mut conn)
///     .await?;
///
/// // For Postgres this would have been inferred to be `Option<i32>`, MySQL `i32`
/// // and SQLite it wouldn't have worked at all because we couldn't know the type.
/// assert_eq!(record.id, MyInt4(1));
/// # }
/// ```
///
/// As mentioned, this allows specifying the type of a pure expression column which is normally
/// forbidden for SQLite as there's no way we can ask SQLite what type the column is expected to be.
///
/// ## Offline Mode (requires the `offline` feature)
/// The macros can be configured to not require a live database connection for compilation,
/// but it requires a couple extra steps:
///
/// * Run `cargo install sqlx-cli`.
/// * In your project with `DATABASE_URL` set (or in a `.env` file) and the database server running,
///   run `cargo sqlx prepare`.
/// * Check the generated `sqlx-data.json` file into version control.
/// * Don't have `DATABASE_URL` set during compilation.
///
/// Your project can now be built without a database connection (you must omit `DATABASE_URL` or
/// else it will still try to connect). To update the generated file simply run `cargo sqlx prepare`
/// again.
///
/// To ensure that your `sqlx-data.json` file is kept up-to-date, both with the queries in your
/// project and your database schema itself, run
/// `cargo install sqlx-cli && cargo sqlx prepare --check` in your Continuous Integration script.
///
/// See [the README for `sqlx-cli`](https://crates.io/crate/sqlx-cli) for more information.
///
/// ## See Also
/// * [query_as!] if you want to use a struct you can name,
/// * [query_file!] if you want to define the SQL query out-of-line,
/// * [query_file_as!] if you want both of the above.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query (
    // by emitting a macro definition from our proc-macro containing the result tokens,
    // we no longer have a need for `proc-macro-hack`
    ($query:literal) => ({
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source = $query);
        }
        macro_result!()
    });
    ($query:literal, $($args:expr),*$(,)?) => ({
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source = $query, args = [$($args),*]);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query!] which does not check the input or output types. This still does parse
/// the query to ensure it's syntactically and semantically valid for the current database.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_unchecked (
    ($query:literal) => ({
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source = $query, checked = false);
        }
        macro_result!()
    });
    ($query:literal, $($args:expr),*$(,)?) => ({
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source = $query, args = [$($args),*], checked = false);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query!] where the SQL query is stored in a separate file.
///
/// Useful for large queries and potentially cleaner than multiline strings.
///
/// The syntax and requirements (see [query!]) are the same except the SQL string is replaced by a
/// file path.
///
/// The file must be relative to the project root (the directory containing `Cargo.toml`),
/// unlike `include_str!()` which uses compiler internals to get the path of the file where it
/// was invoked.
///
/// -----
///
/// `examples/queries/account-by-id.sql`:
/// ```text
/// select * from (select (1) as id, 'Herp Derpinson' as name) accounts
/// where id = ?
/// ```
///
/// `src/my_query.rs`:
/// ```rust,ignore
/// # use sqlx::Connect;
/// # #[cfg(all(feature = "mysql", feature = "runtime-async-std"))]
/// # #[async_std::main]
/// # async fn main() -> sqlx::Result<()>{
/// # let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
/// #
/// # if !(db_url.starts_with("mysql") || db_url.starts_with("mariadb")) { return Ok(()) }
/// # let mut conn = sqlx::MySqlConnection::connect(db_url).await?;
/// let account = sqlx::query_file!("tests/test-query-account-by-id.sql", 1i32)
///     .fetch_one(&mut conn)
///     .await?;
///
/// println!("{:?}", account);
/// println!("{}: {}", account.id, account.name);
///
/// # Ok(())
/// # }
/// #
/// # #[cfg(any(not(feature = "mysql"), not(feature = "runtime-async-std")))]
/// # fn main() {}
/// ```
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_file (
    ($path:literal) => (#[allow(dead_code)]{
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source_file = $path);
        }
        macro_result!()
    });
    ($path:literal, $($args:expr),*$(,)?) => (#[allow(dead_code)]{
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(source_file = $path, args = [$($args),*]);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query_file!] which does not check the input or output types. This still does parse
/// the query to ensure it's syntactically and semantically valid for the current database.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_file_unchecked (
    ($path:literal) => (#[allow(dead_code)]{
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::query_file_unchecked!(source_file = $path, checked = false);
        }
        macro_result!()
    });
    ($path:literal, $($args:expr),*$(,)?) => (#[allow(dead_code)]{
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::query_file_unchecked!(source_file = $path, args = [$($args),*], checked = false);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query!] which takes a path to an explicitly defined struct as the output type.
///
/// This lets you return the struct from a function or add your own trait implementations.
///
/// No trait implementations are required; the macro maps rows using a struct literal
/// where the names of columns in the query are expected to be the same as the fields of the struct
/// (but the order does not need to be the same). The types of the columns are based on the
/// query and not the corresponding fields of the struct, so this is type-safe as well.
///
/// This enforces a few things:
/// * The query must output at least one column.
/// * The column names of the query must match the field names of the struct.
/// * Neither the query nor the struct may have unused fields.
///
/// The only modification to the syntax is that the struct name is given before the SQL string:
/// ```rust,ignore
/// # use sqlx::Connect;
/// # #[cfg(all(feature = "mysql", feature = "runtime-async-std"))]
/// # #[async_std::main]
/// # async fn main() -> sqlx::Result<()>{
/// # let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
/// #
/// # if !(db_url.starts_with("mysql") || db_url.starts_with("mariadb")) { return Ok(()) }
/// # let mut conn = sqlx::MySqlConnection::connect(db_url).await?;
/// #[derive(Debug)]
/// struct Account {
///     id: i32,
///     name: String
/// }
///
/// // let mut conn = <impl sqlx::Executor>;
/// let account = sqlx::query_as!(
///         Account,
///         "select * from (select (1) as id, 'Herp Derpinson' as name) accounts where id = ?",
///         1i32
///     )
///     .fetch_one(&mut conn)
///     .await?;
///
/// println!("{:?}", account);
/// println!("{}: {}", account.id, account.name);
///
/// # Ok(())
/// # }
/// #
/// # #[cfg(any(not(feature = "mysql"), not(feature = "runtime-async-std")))]
/// # fn main() {}
/// ```
///
/// ## Nullability
/// Use `Option` for columns which may be `NULL` in order to avoid a runtime error being returned
/// from `.fetch_*()`.
///
/// ### Additional Column Type Override Option
/// In addition to the column type overrides supported by [query!], `query_as!()` supports an
/// additional override option:
///
/// If you select a column `foo as "foo: _"` (Postgres/SQLite) or `` foo as `foo: _` `` (MySQL)
/// it causes that column to be inferred based on the type of the corresponding field in the given
/// record struct. Runtime type-checking is still done so an error will be emitted if the types
/// are not compatible.
///
/// This allows you to override the inferred type of a column to instead use a custom-defined type:
///
/// ```rust,ignore
/// #[derive(sqlx::Type)]
/// #[sqlx(transparent)
/// struct MyInt4(i32);
///
/// struct Record {
///     id: MyInt4,
/// }
///
/// let my_int = MyInt4(1);
///
/// // Postgres/SQLite
/// sqlx::query!(r#"select 1 as "id: _""#) // MySQL: use "select 1 as `id: _`" instead
///     .fetch_one(&mut conn)
///     .await?;
///
/// assert_eq!(record.id, MyInt4(1));
/// ```
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_as (
    ($out_struct:path, $query:literal) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query);
        }
        macro_result!()
    });
    ($out_struct:path, $query:literal, $($args:expr),*$(,)?) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query, args = [$($args),*]);
        }
        macro_result!($($args),*)
    })
);

/// Combines the syntaxes of [query_as!] and [query_file!].
///
/// Enforces requirements of both macros; see them for details.
///
/// ```rust,ignore
/// # use sqlx::Connect;
/// # #[cfg(all(feature = "mysql", feature = "runtime-async-std"))]
/// # #[async_std::main]
/// # async fn main() -> sqlx::Result<()>{
/// # let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
/// #
/// # if !(db_url.starts_with("mysql") || db_url.starts_with("mariadb")) { return Ok(()) }
/// # let mut conn = sqlx::MySqlConnection::connect(db_url).await?;
/// #[derive(Debug)]
/// struct Account {
///     id: i32,
///     name: String
/// }
///
/// // let mut conn = <impl sqlx::Executor>;
/// let account = sqlx::query_file_as!(Account, "tests/test-query-account-by-id.sql", 1i32)
///     .fetch_one(&mut conn)
///     .await?;
///
/// println!("{:?}", account);
/// println!("{}: {}", account.id, account.name);
///
/// # Ok(())
/// # }
/// #
/// # #[cfg(any(not(feature = "mysql"), not(feature = "runtime-async-std")))]
/// # fn main() {}
/// ```
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_file_as (
    ($out_struct:path, $path:literal) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source_file = $path);
        }
        macro_result!()
    });
    ($out_struct:path, $path:literal, $($args:tt),*$(,)?) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source_file = $path, args = [$($args),*]);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query_as!] which does not check the input or output types. This still does parse
/// the query to ensure it's syntactically and semantically valid for the current database.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_as_unchecked (
    ($out_struct:path, $query:literal) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query, checked = false);
        }
        macro_result!()
    });

    ($out_struct:path, $query:literal, $($args:expr),*$(,)?) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query, args = [$($args),*], checked = false);
        }
        macro_result!($($args),*)
    })
);

/// A variant of [query_file_as!] which does not check the input or output types. This
/// still does parse the query to ensure it's syntactically and semantically valid
/// for the current database.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! query_file_as_unchecked (
    ($out_struct:path, $path:literal) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::query_file_as_unchecked!(record = $out_struct, source_file = $path, checked = false);
        }
        macro_result!()
    });

    ($out_struct:path, $path:literal, $($args:tt),*$(,)?) => (#[allow(dead_code)] {
        #[macro_use]
        mod _macro_result {
            $crate::sqlx_macros::query_file_as_unchecked!(record = $out_struct, source_file = $path, args = [$($args),*], checked = false);
        }
        macro_result!($($args),*)
    })
);
