use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::TryRngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// A freshly created session. `raw_id` goes into the cookie and is never stored.
pub struct NewSession {
    pub raw_id: String,
    pub csrf_token: String,
    pub expires_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
pub struct Session {
    pub user_id: Uuid,
    pub csrf_token: String,
    pub id_token: Option<String>,
}

/// 256-bit random token from the OS RNG, base64url encoded.
pub fn random_token() -> anyhow::Result<String> {
    let mut buf = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|e| anyhow::anyhow!("OS RNG failure: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

pub fn hash_id(raw_id: &str) -> String {
    Sha256::digest(raw_id.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    id_token: Option<&str>,
    user_agent: Option<&str>,
    ttl_hours: i64,
) -> anyhow::Result<NewSession> {
    let session = NewSession {
        raw_id: random_token()?,
        csrf_token: random_token()?,
        expires_at: OffsetDateTime::now_utc() + time::Duration::hours(ttl_hours),
    };
    let user_agent = user_agent.map(|ua| ua.chars().take(256).collect::<String>());
    sqlx::query(
        "INSERT INTO sessions (id_hash, user_id, csrf_token, id_token, user_agent, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash_id(&session.raw_id))
    .bind(user_id)
    .bind(&session.csrf_token)
    .bind(id_token)
    .bind(user_agent)
    .bind(session.expires_at)
    .execute(pool)
    .await?;
    Ok(session)
}

/// Unexpired session for the raw cookie value.
pub async fn find_valid(pool: &PgPool, raw_id: &str) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as::<_, Session>(
        "SELECT user_id, csrf_token, id_token FROM sessions
         WHERE id_hash = $1 AND expires_at > now()",
    )
    .bind(hash_id(raw_id))
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, raw_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id_hash = $1")
        .bind(hash_id(raw_id))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_for_user(pool: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn delete_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Hourly cleanup of expired sessions; the first run happens immediately.
pub fn spawn_cleanup(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match delete_expired(&pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(deleted = n, "Expired sessions removed"),
                Err(e) => tracing::warn!("Session cleanup failed: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(
            hash_id("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn random_tokens_differ() {
        let a = random_token().unwrap();
        let b = random_token().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
    }
}
