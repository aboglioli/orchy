use tower_cookies::Cookies;
use tower_cookies::cookie::SameSite;

pub const AUTH_COOKIE_NAME: &str = "orchy_token";

pub struct CookieConfig {
    pub secure: bool,
    pub same_site: SameSite,
    pub max_age_hours: i64,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            secure: false,
            same_site: SameSite::Lax,
            max_age_hours: 24,
        }
    }
}

pub fn set_auth_cookie(cookies: &Cookies, token: &str, config: &CookieConfig) {
    let cookie = tower_cookies::Cookie::build((AUTH_COOKIE_NAME, token.to_string()))
        .http_only(true)
        .secure(config.secure)
        .same_site(config.same_site)
        .path("/")
        .max_age(tower_cookies::cookie::time::Duration::hours(
            config.max_age_hours,
        ))
        .build();

    cookies.add(cookie);
}

pub fn clear_auth_cookie(cookies: &Cookies) {
    let cookie = tower_cookies::Cookie::build((AUTH_COOKIE_NAME, ""))
        .http_only(true)
        .path("/")
        .max_age(tower_cookies::cookie::time::Duration::seconds(-1))
        .build();

    cookies.add(cookie);
}

pub fn get_auth_token(cookies: &Cookies) -> Option<String> {
    cookies.get(AUTH_COOKIE_NAME).map(|c| c.value().to_string())
}
