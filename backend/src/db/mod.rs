pub mod access;
pub mod auth;
pub mod folders;
pub mod notes;
pub mod schema;
pub mod sharing;
pub mod users;

use crate::error::Error;
use diesel::{
    Connection, SqliteConnection,
    connection::SimpleConnection,
    r2d2::{self, ConnectionManager},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

#[derive(Debug)]
struct SqliteSetup;

impl r2d2::CustomizeConnection<SqliteConnection, r2d2::Error> for SqliteSetup {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        conn.batch_execute("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
            .map_err(r2d2::Error::QueryError)
    }
}

pub fn connect(url: &str) -> std::io::Result<DbPool> {
    let mut conn = SqliteConnection::establish(url)
        .map_err(|_| std::io::Error::other("Cannot open SQLite database"))?;
    conn.batch_execute(
        "PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
    )
    .map_err(|_| std::io::Error::other("Cannot configure SQLite"))?;

    conn.immediate_transaction::<_, diesel::result::Error, _>(|conn| {
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|_| diesel::result::Error::RollbackTransaction)?;

        Ok(())
    })
    .map_err(|_| std::io::Error::other("Database migration failed"))?;
    r2d2::Pool::builder()
        .max_size(8)
        .connection_customizer(Box::new(SqliteSetup))
        .build(ConnectionManager::new(url))
        .map_err(|_| std::io::Error::other("Cannot create database pool"))
}

/// Keep connection checkout, queries, and whole transactions off the async executor.
pub async fn run<T: Send + 'static>(
    pool: DbPool,
    operation: impl FnOnce(&mut SqliteConnection) -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    actix_web::web::block(move || {
        let mut conn = pool.get().map_err(|_| Error::Unavailable)?;
        operation(&mut conn)
    })
    .await
    .map_err(|_| Error::Internal)?
}

#[cfg(test)]
mod tests {
    use crate::{
        db,
        db::schema::{email_challenges as c, notes as n, users as u},
        models::{email_challenge::EmailChallenge, user::User},
        utils::crypto,
    };
    use diesel::{connection::SimpleConnection, prelude::*};

    #[actix_web::test]
    async fn migrations_constraints_and_reopen_preserve_records() {
        let database = crate::test_utils::TestDb::new();
        let pool = &database.pool;
        let now = crypto::now();
        {
            let mut conn = pool.get().unwrap();
            for (id, verified, created_at) in [
                ("active", true, now - 100000),
                ("abandoned", false, now - 86401),
                ("pending", false, now),
            ] {
                db::auth::insert_user(
                    &mut conn,
                    &User {
                        id: id.into(),
                        email: format!("{id}@example.test"),
                        password_hash: "hash".into(),
                        token_version: 0,
                        verified,
                        created_at,
                        name: "Test user".into(),
                        name_key: "test user".into(),
                    },
                )
                .unwrap();
            }
            for (id, expires_at) in [("expired", now), ("live", now + 600)] {
                db::auth::replace_challenge(
                    &mut conn,
                    &EmailChallenge {
                        id: id.into(),
                        user_id: if id == "expired" { "pending" } else { "active" }.into(),
                        purpose: "signin".into(),
                        code_hash: "hash".into(),
                        expires_at,
                        attempts: 0,
                        last_sent_at: now,
                        created_at: now,
                    },
                )
                .unwrap();
            }

            // Foreign-key enforcement is configured on every pooled connection.
            assert!(
                diesel::insert_into(n::table)
                    .values((
                        n::id.eq("bad"),
                        n::user_id.eq("missing"),
                        n::title.eq(""),
                        n::body.eq(""),
                        n::created_at.eq(now),
                        n::updated_at.eq(now)
                    ))
                    .execute(&mut conn)
                    .is_err()
            );
        }
        let reopened = database.reopen();
        let mut conn = reopened.get().unwrap();
        assert_eq!(u::table.count().get_result::<i64>(&mut conn).unwrap(), 3);
        assert_eq!(
            c::table
                .order(c::id)
                .select(c::id)
                .load::<String>(&mut conn)
                .unwrap(),
            ["expired", "live"]
        );
        // Startup applies the initial schema with a refresh blacklist and no session table.
        assert!(conn.batch_execute("SELECT * FROM sessions").is_err());
        assert!(conn.batch_execute("SELECT * FROM token_blacklist").is_ok());

        assert_eq!(
            db::auth::user(&mut conn, "active").unwrap().token_version,
            0
        );
        db::auth::update_password(&mut conn, "active", "first-hash").unwrap();
        db::auth::update_password(&mut conn, "active", "second-hash").unwrap();
        assert_eq!(
            db::auth::user(&mut conn, "active").unwrap().token_version,
            2
        );

        // Database guards reject duplicate addresses and invalid challenge purposes.
        assert!(
        conn.batch_execute(
            "INSERT INTO users (id, email, password_hash, verified, created_at) VALUES ('duplicate', 'active@example.test', 'hash', 1, 0)"
        )
        .is_err()
    );
        assert!(
            conn.batch_execute(
                "INSERT INTO email_challenges VALUES ('bad', 'active', 'other', 'hash', 0, 0, 0, 0)"
            )
            .is_err()
        );
    }

    #[test]
    fn initial_schema_indexes_notes_and_preserves_them_on_reopen() {
        use diesel_migrations::MigrationHarness;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("upgrade.sqlite");
        let url = path.to_str().unwrap();
        {
            let mut conn = SqliteConnection::establish(url).unwrap();
            // Install the initial schema, then insert notes before reopening the database.
            let migrations =
                diesel::migration::MigrationSource::<diesel::sqlite::Sqlite>::migrations(
                    &db::MIGRATIONS,
                )
                .unwrap();
            diesel::migration::MigrationConnection::setup(&mut conn).unwrap();
            assert_eq!(migrations.len(), 1);
            conn.run_migrations(&migrations).unwrap();
            conn.batch_execute("INSERT INTO users(id,email,password_hash,verified,created_at) VALUES ('owner','owner@test.test','hash',1,123);
                INSERT INTO notes(id,user_id,title,body,created_at,updated_at) VALUES ('old','owner','Legacy title','legacy searchable content',123,456);").unwrap();
        }
        let pool = db::connect(url).unwrap();
        let mut conn = pool.get().unwrap();
        let note = db::notes::get(&mut conn, "owner", "old").unwrap();
        assert_eq!(note.body, "legacy searchable content");
        assert_eq!(note.created_at, 123);
        assert_eq!(note.updated_at, 456);
        assert_eq!(note.revision, 1);
        assert_eq!(note.folder_id, None);
        assert!(db::auth::user(&mut conn, "owner").unwrap().name.is_empty());
        let found = db::notes::search(
            &mut conn,
            "owner",
            "\"searchable\"*",
            "title : \"searchable\"",
            50,
            0,
        )
        .unwrap();
        assert_eq!(found[0].id, "old");
    }
}
