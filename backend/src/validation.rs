use crate::error::AppError;

/// A recipe's source URL is rendered into an `<a href>` in the client, so a
/// stored `javascript:` or `data:` URL would execute on click. Enforced in
/// `db::recipes::{create,update,create_discovered}` — the actual write sink —
/// so a future second caller can't reach the database while bypassing it.
///
/// An empty string is accepted: it is the "clear this field" sentinel that
/// `db::recipes::update` understands.
pub fn check_source_url(value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Ok(());
    }
    match reqwest::Url::parse(value) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => Ok(()),
        _ => Err(AppError::BadRequest(
            "source_url musí být absolutní http(s) odkaz".into(),
        )),
    }
}

/// Trims an incoming `source_url` before validating it, so a whitespace-only
/// value (e.g. a stray space left over from an edit to some other field)
/// collapses to the `""` clear-sentinel instead of reaching `Url::parse` and
/// 400ing on a field the caller may not have touched. Returns the trimmed
/// value so callers store what was actually validated.
pub fn normalize_source_url(value: Option<&str>) -> Result<Option<&str>, AppError> {
    let trimmed = value.map(str::trim);
    if let Some(url) = trimmed {
        check_source_url(url)?;
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{check_source_url, normalize_source_url};

    #[test]
    fn accepts_http_https_and_empty() {
        assert!(check_source_url("").is_ok());
        assert!(check_source_url("http://vareni.cz/r/1").is_ok());
        assert!(check_source_url("https://vareni.cz/r/1").is_ok());
    }

    #[test]
    fn rejects_other_schemes_and_garbage() {
        assert!(check_source_url("javascript:alert(1)").is_err());
        assert!(check_source_url("JavaScript:alert(1)").is_err());
        assert!(check_source_url("data:text/html,<script>").is_err());
        assert!(check_source_url("file:///etc/passwd").is_err());
        assert!(check_source_url("not-a-url").is_err());
    }

    #[test]
    fn normalize_trims_and_collapses_whitespace_to_clear_sentinel() {
        assert_eq!(normalize_source_url(None).unwrap(), None);
        assert_eq!(normalize_source_url(Some("   ")).unwrap(), Some(""));
        assert_eq!(
            normalize_source_url(Some("  https://vareni.cz/r/1  ")).unwrap(),
            Some("https://vareni.cz/r/1")
        );
        assert!(normalize_source_url(Some("  javascript:alert(1)  ")).is_err());
    }
}
