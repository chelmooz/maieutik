//! Politique de sortie réseau : local-first strict. L'UI ne parle qu'à la
//! boucle locale, à un socket Unix ou à un processus fils (stdio).

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EgressError {
    #[error("URL invalide : {0}")]
    Invalid(String),
    #[error("destination non locale refusée : {0}")]
    NotLocal(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EgressPolicy {
    /// `host.docker.internal` n'est accepté que si l'UI tourne dans un conteneur.
    pub allow_docker_host: bool,
}

impl EgressPolicy {
    pub fn check_url(&self, url: &str) -> Result<(), EgressError> {
        let rest = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .ok_or_else(|| EgressError::Invalid(url.to_owned()))?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // `http://127.0.0.1@evil.com/` : l'hôte réel est après '@'.
        let hostport = authority.rsplit('@').next().unwrap_or("");
        let host = if let Some(v6) = hostport.strip_prefix('[') {
            v6.split(']').next().unwrap_or("")
        } else {
            hostport.split(':').next().unwrap_or("")
        };
        if host.is_empty() {
            return Err(EgressError::Invalid(url.to_owned()));
        }
        if self.is_local_host(host) {
            Ok(())
        } else {
            Err(EgressError::NotLocal(host.to_owned()))
        }
    }

    fn is_local_host(&self, host: &str) -> bool {
        let h = host.to_ascii_lowercase();
        if h == "localhost" || h == "::1" {
            return true;
        }
        if self.allow_docker_host && h == "host.docker.internal" {
            return true;
        }
        let parts: Vec<&str> = h.split('.').collect();
        parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) && parts[0] == "127"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(u: &str) -> bool {
        EgressPolicy::default().check_url(u).is_ok()
    }

    #[test]
    fn loopback_is_allowed() {
        assert!(ok("http://localhost:11434/v1"));
        assert!(ok("http://127.0.0.1:8080/x"));
        assert!(ok("https://127.1.2.3/"));
        assert!(ok("http://[::1]:9000/"));
    }

    #[test]
    fn remote_hosts_are_refused() {
        assert!(!ok("http://example.com/"));
        assert!(!ok("https://api.openai.com/v1"));
        assert!(!ok("http://192.168.1.10:11434/"));
        assert!(!ok("http://10.0.0.1/"));
    }

    #[test]
    fn lookalike_hosts_are_refused() {
        assert!(!ok("http://localhost.evil.com/"));
        assert!(!ok("http://127.0.0.1.evil.com/"));
        assert!(!ok("http://127.0.0.1@evil.com/"));
        assert!(!ok("http://127.0.0.256/"));
    }

    #[test]
    fn docker_host_only_when_enabled() {
        let url = "http://host.docker.internal:11434/v1";
        assert!(!ok(url));
        assert!(EgressPolicy { allow_docker_host: true }.check_url(url).is_ok());
    }

    #[test]
    fn non_http_and_empty_are_invalid() {
        assert!(matches!(EgressPolicy::default().check_url("ftp://localhost/"), Err(EgressError::Invalid(_))));
        assert!(matches!(EgressPolicy::default().check_url("http:///x"), Err(EgressError::Invalid(_))));
    }
}
