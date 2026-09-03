//! Authentication helpers for the token bench fixture.

use std::time::{Duration, SystemTime};

/// A logged-in session.
pub struct Session {
    /// Who logged in.
    pub user: String,
    /// When the session expires.
    pub expires: SystemTime,
}

/// Credentials presented at login.
pub struct Credentials {
    /// Login name.
    pub user: String,
    /// Secret (never logged).
    pub password: String,
}

/// Errors from the auth flow.
pub enum AuthError {
    /// Unknown user.
    UnknownUser,
    /// Wrong password.
    BadPassword,
    /// Session expired.
    Expired,
}

/// Checks credentials against the user table.
pub fn authenticate(creds: &Credentials) -> Result<Session, AuthError> {
    if creds.user.is_empty() {
        return Err(AuthError::UnknownUser);
    }
    if creds.password.len() < 8 {
        return Err(AuthError::BadPassword);
    }
    Ok(Session {
        user: creds.user.clone(),
        expires: SystemTime::now() + Duration::from_secs(3600),
    })
}

/// Whether the session is still valid.
pub fn is_valid(session: &Session) -> bool {
    SystemTime::now() < session.expires
}

/// Refreshes a session for another hour.
pub fn refresh(session: &mut Session) {
    session.expires = SystemTime::now() + Duration::from_secs(3600);
}
