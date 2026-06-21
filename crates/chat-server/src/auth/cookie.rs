use cookie::{Cookie, SameSite, time::Duration};
use url::Url;

const SESSION_COOKIE_HTTPS: &str = "__Host-chat_session";
const SESSION_COOKIE_HTTP: &str = "chat_session";
const LOGIN_COOKIE_HTTPS: &str = "__Host-chat_login";
const LOGIN_COOKIE_HTTP: &str = "chat_login";
const SESSION_DAYS: i64 = 30;
const LOGIN_MINUTES: i64 = 10;

#[derive(Clone, Debug)]
pub(crate) struct CookiePolicy {
    secure: bool,
}

impl CookiePolicy {
    pub(crate) fn new(public_url: &Url) -> Self {
        Self {
            secure: public_url.scheme() == "https",
        }
    }

    pub(crate) fn session_name(&self) -> &'static str {
        if self.secure {
            SESSION_COOKIE_HTTPS
        } else {
            SESSION_COOKIE_HTTP
        }
    }

    pub(crate) fn login_name(&self) -> &'static str {
        if self.secure {
            LOGIN_COOKIE_HTTPS
        } else {
            LOGIN_COOKIE_HTTP
        }
    }

    pub(crate) fn session_cookie(&self, value: String) -> Cookie<'static> {
        self.build(self.session_name(), value, Duration::days(SESSION_DAYS))
    }

    pub(crate) fn login_cookie(&self, value: String) -> Cookie<'static> {
        self.build(self.login_name(), value, Duration::minutes(LOGIN_MINUTES))
    }

    pub(crate) fn remove_session_cookie(&self) -> Cookie<'static> {
        self.removal(self.session_name())
    }

    pub(crate) fn remove_login_cookie(&self) -> Cookie<'static> {
        self.removal(self.login_name())
    }

    pub(crate) fn find<'a>(&self, header: &'a str, name: &str) -> Option<String> {
        Cookie::split_parse(header)
            .filter_map(Result::ok)
            .find(|cookie| cookie.name() == name)
            .map(|cookie| cookie.value().to_owned())
    }

    fn build(&self, name: &'static str, value: String, max_age: Duration) -> Cookie<'static> {
        Cookie::build((name, value))
            .http_only(true)
            .secure(self.secure)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(max_age)
            .build()
    }

    fn removal(&self, name: &'static str) -> Cookie<'static> {
        let mut cookie = self.build(name, String::new(), Duration::ZERO);
        cookie.make_removal();
        cookie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_session_cookie_has_host_prefix_and_security_attributes() {
        let url = Url::parse("https://chat.example.com").expect("URL is valid");
        let cookie = CookiePolicy::new(&url).session_cookie(String::from("token"));

        assert_eq!(cookie.name(), SESSION_COOKIE_HTTPS);
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.domain(), None);
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
    }

    #[test]
    fn loopback_cookie_is_not_marked_secure_and_can_be_parsed() {
        let url = Url::parse("http://127.0.0.1:3000").expect("URL is valid");
        let policy = CookiePolicy::new(&url);
        let cookie = policy.session_cookie(String::from("token"));

        assert_eq!(cookie.name(), SESSION_COOKIE_HTTP);
        assert_eq!(cookie.secure(), Some(false));
        assert_eq!(
            policy.find("a=b; chat_session=token", cookie.name()),
            Some(String::from("token"))
        );
    }
}
