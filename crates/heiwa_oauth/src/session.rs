//! Converting a provider's token response into storable form.
//!
//! This module owns exactly one hazard: a refresh response does not repeat the
//! refresh token. Storing a refreshed response directly would drop it, and the
//! connector would keep working until the next expiry and then be unable to
//! renew — a failure that appears an hour after the change that caused it.
//!
//! Deliberately no clock and no policy. `now_unix` is a parameter so expiry
//! arithmetic is testable, and *when* to refresh stays with the caller, which
//! already has `heiwa_provider::oauth::needs_refresh`.

use heiwa_vault::OAuthSecret;

use crate::token::TokenResponse;

/// Convert a first-authorization response into a storable secret.
pub fn to_secret(response: &TokenResponse, now_unix: u64) -> OAuthSecret {
    OAuthSecret {
        access_token: response.access_token.clone(),
        refresh_token: response.refresh_token.clone(),
        expires_at_unix: response.expires_in.map(|ttl| now_unix.saturating_add(ttl)),
        scope: response.scope.clone(),
    }
}

/// Fold a refresh response onto the secret already held.
///
/// Keeps the stored refresh token whenever the response omits one, and keeps
/// the stored scope for the same reason — providers commonly return neither on
/// refresh, and treating absence as revocation loses information the user
/// granted.
pub fn merge_refreshed(
    existing: &OAuthSecret,
    response: &TokenResponse,
    now_unix: u64,
) -> OAuthSecret {
    OAuthSecret {
        access_token: response.access_token.clone(),
        refresh_token: response
            .refresh_token
            .clone()
            .or_else(|| existing.refresh_token.clone()),
        expires_at_unix: response.expires_in.map(|ttl| now_unix.saturating_add(ttl)),
        scope: response.scope.clone().or_else(|| existing.scope.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(access: &str, refresh: Option<&str>, ttl: Option<u64>) -> TokenResponse {
        TokenResponse {
            access_token: access.into(),
            refresh_token: refresh.map(str::to_string),
            expires_in: ttl,
            token_type: Some("Bearer".into()),
            scope: None,
        }
    }

    fn stored() -> OAuthSecret {
        OAuthSecret {
            access_token: "old-access".into(),
            refresh_token: Some("the-only-refresh-token".into()),
            expires_at_unix: Some(1_000),
            scope: Some("calendar.readonly".into()),
        }
    }

    #[test]
    fn first_authorization_records_an_absolute_expiry() {
        let secret = to_secret(&response("at", Some("rt"), Some(3599)), 1_700_000_000);
        assert_eq!(secret.expires_at_unix, Some(1_700_003_599));
        assert_eq!(secret.refresh_token.as_deref(), Some("rt"));
    }

    #[test]
    fn a_response_without_expiry_is_stored_as_long_lived() {
        // needs_refresh treats None as "no expiry known", so inventing one here
        // would force refreshes the provider never asked for.
        let secret = to_secret(&response("at", Some("rt"), None), 1_700_000_000);
        assert_eq!(secret.expires_at_unix, None);
    }

    #[test]
    fn refresh_keeps_the_stored_refresh_token_when_the_response_omits_it() {
        // The whole reason this module exists.
        let merged = merge_refreshed(&stored(), &response("new-access", None, Some(3599)), 2_000);
        assert_eq!(merged.access_token, "new-access");
        assert_eq!(
            merged.refresh_token.as_deref(),
            Some("the-only-refresh-token"),
            "dropping this would make the account unrenewable at the next expiry"
        );
        assert_eq!(merged.expires_at_unix, Some(5_599));
    }

    #[test]
    fn refresh_adopts_a_rotated_refresh_token_when_one_is_returned() {
        // Providers that rotate on every refresh must not be pinned to the old
        // token, or the next renewal fails.
        let merged = merge_refreshed(
            &stored(),
            &response("new-access", Some("rotated"), Some(60)),
            2_000,
        );
        assert_eq!(merged.refresh_token.as_deref(), Some("rotated"));
    }

    #[test]
    fn refresh_keeps_the_stored_scope_when_the_response_omits_it() {
        let merged = merge_refreshed(&stored(), &response("new-access", None, Some(60)), 2_000);
        assert_eq!(merged.scope.as_deref(), Some("calendar.readonly"));
    }

    #[test]
    fn expiry_arithmetic_saturates_instead_of_wrapping() {
        // A hostile or broken provider returning a huge ttl must not wrap the
        // expiry to a past instant, which would look like a permanently
        // expired token.
        let merged = merge_refreshed(&stored(), &response("at", None, Some(u64::MAX)), 10);
        assert_eq!(merged.expires_at_unix, Some(u64::MAX));
    }
}
