//! Claves de tarea en mensajes de commit y nombres de rama (RF-INT-04, D16).

/// Una tarea mencionada: su clave en mayúsculas y si el texto la cierra
/// (`fixes API-42`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub key: String,
    pub closes: bool,
}

/// Palabras que, delante de una clave, la cierran.
const CLOSING: &[&str] = &[
    "fix", "fixes", "fixed", "close", "closes", "closed", "resolve", "resolves", "resolved",
    "arregla", "cierra", "resuelve",
];

/// Palabras que mantienen el cierre en una lista: `closes API-1 and API-2`.
const JOINERS: &[&str] = &["and", "y", "e"];

/// `API-42`, `api-42` o `API-42-login` → `API-42`: una clave de proyecto
/// (letra y de 1 a 9 letras o dígitos), guion y número.
fn key_of(token: &str) -> Option<String> {
    let (project, rest) = token.split_once('-')?;
    let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[number.len()..];
    let valid_project = (2..=10).contains(&project.len())
        && project.starts_with(|c: char| c.is_ascii_alphabetic())
        && project.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid_project || number.is_empty() || !(after.is_empty() || after.starts_with('-')) {
        return None;
    }
    Some(format!("{}-{number}", project.to_ascii_uppercase()))
}

/// Tareas mencionadas en un texto, en orden y sin repetir. Una clave tras
/// una palabra de cierre (o tras otra cerrada, unida por coma o `and`) se
/// marca como cerrada.
pub fn references(text: &str) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    let mut closing = false;
    for token in text
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|t| !t.is_empty())
    {
        if let Some(key) = key_of(token) {
            match out.iter_mut().find(|r| r.key == key) {
                Some(r) => r.closes |= closing,
                None => out.push(Reference {
                    key,
                    closes: closing,
                }),
            }
            continue;
        }
        let word = token.to_lowercase();
        if CLOSING.contains(&word.as_str()) {
            closing = true;
        } else if !JOINERS.contains(&word.as_str()) {
            closing = false;
        }
    }
    out
}

/// Primera línea de un mensaje de commit.
pub fn subject(message: &str) -> &str {
    message.lines().next().unwrap_or_default().trim()
}

/// URL web de un commit a partir del remoto (`https://host/dueño/repo(.git)`
/// o `git@host:dueño/repo(.git)`); `None` si no tiene esa forma.
pub fn commit_url(remote: &str, sha: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    let base = if let Some(rest) = remote.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        format!("https://{host}/{path}")
    } else if remote.starts_with("https://") || remote.starts_with("http://") {
        // Sin credenciales en la URL guardada.
        match remote.split_once("://") {
            Some((scheme, rest)) => {
                let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
                format!("{scheme}://{rest}")
            }
            None => return None,
        }
    } else {
        return None;
    };
    let path = base.splitn(4, '/').nth(3)?;
    if path.split('/').filter(|s| !s.is_empty()).count() < 2 {
        return None;
    }
    Some(format!("{base}/commit/{sha}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Vec<(String, bool)> {
        references(text)
            .into_iter()
            .map(|r| (r.key, r.closes))
            .collect()
    }

    #[test]
    fn finds_keys_and_closing_words() {
        assert_eq!(keys("Fixes API-42: login roto"), [("API-42".into(), true)]);
        assert_eq!(
            keys("refactor del parser (ver api-7)"),
            [("API-7".into(), false)]
        );
        assert_eq!(
            keys("closes API-1, API-2 and WEB-3; toca API-4"),
            [
                ("API-1".into(), true),
                ("API-2".into(), true),
                ("WEB-3".into(), true),
                ("API-4".into(), false),
            ]
        );
        assert_eq!(keys("cierra API-9"), [("API-9".into(), true)]);
        assert_eq!(keys("fix(api): maneja nulos"), []);
        assert_eq!(
            keys("API-5 primero, luego resuelve API-5"),
            [("API-5".into(), true)]
        );
        assert_eq!(keys("a-2 X-1 API-x 1API-2 API-3x"), []);
    }

    #[test]
    fn branch_names_carry_keys() {
        assert_eq!(keys("feat/API-42-login"), [("API-42".into(), false)]);
        assert_eq!(keys("api-42"), [("API-42".into(), false)]);
        assert_eq!(keys("main"), []);
    }

    #[test]
    fn commit_urls_from_common_remotes() {
        assert_eq!(
            commit_url("git@github.com:org/repo.git", "abc").as_deref(),
            Some("https://github.com/org/repo/commit/abc")
        );
        assert_eq!(
            commit_url("https://user:pw@gitlab.com/g/sub/repo.git", "abc").as_deref(),
            Some("https://gitlab.com/g/sub/repo/commit/abc")
        );
        assert_eq!(commit_url("/srv/git/repo.git", "abc"), None);
        assert_eq!(commit_url("https://host/solo", "abc"), None);
        assert_eq!(subject("Título\n\ncuerpo"), "Título");
    }
}
