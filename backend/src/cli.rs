//! Break-glass admin commands: `cooking-app create-session <user-name>` and
//! `cooking-app revoke-sessions <user-name>`. Need only DATABASE_URL (+ cookie env).

use sqlx::PgPool;

use crate::auth::session_cookie_name;
use crate::config::Config;
use crate::db;

const USAGE: &str = "usage: cooking-app [create-session <user-name> | revoke-sessions <user-name>]";

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let (command, name) = match args {
        [command, name] => (command.as_str(), name.as_str()),
        _ => anyhow::bail!(USAGE),
    };
    let config = Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;
    let user = db::users::find_by_name(&pool, name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user '{name}' not found"))?;

    match command {
        "create-session" => {
            let session = db::sessions::create(
                &pool,
                user.id,
                None,
                Some("cli create-session"),
                config.session_ttl_hours,
            )
            .await?;
            println!(
                "Session for '{}' valid until {}",
                user.name, session.expires_at
            );
            println!(
                "Cookie:     {}={}",
                session_cookie_name(&config),
                session.raw_id
            );
            println!("CSRF token: {} (send as X-CSRF-Token)", session.csrf_token);
            if config.oidc.is_none() {
                println!("Warning: OIDC is not configured, the server rejects all sessions.");
            }
        }
        "revoke-sessions" => {
            let n = db::sessions::delete_for_user(&pool, user.id).await?;
            println!("Revoked {n} session(s) of '{}'", user.name);
        }
        _ => anyhow::bail!(USAGE),
    }
    Ok(())
}
