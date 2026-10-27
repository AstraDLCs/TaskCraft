use crate::RuleError;

/// Normaliza y valida una clave de proyecto (`api` → `API`).
pub fn project_key(raw: &str) -> Result<String, RuleError> {
    let key = raw.trim().to_uppercase();
    let mut chars = key.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest_ok = chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if (2..=10).contains(&key.len()) && starts_with_letter && rest_ok {
        Ok(key)
    } else {
        Err(RuleError::ProjectKey(raw.to_owned()))
    }
}

pub fn task_title(raw: &str) -> Result<String, RuleError> {
    let title = raw.trim();
    if title.is_empty() || title.chars().count() > 200 {
        Err(RuleError::Title)
    } else {
        Ok(title.to_owned())
    }
}

pub fn label_name(raw: &str) -> Result<String, RuleError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 40 || name.contains(',') {
        Err(RuleError::LabelName)
    } else {
        Ok(name.to_owned())
    }
}

pub fn label_color(raw: &str) -> Result<String, RuleError> {
    let color = raw.trim().to_lowercase();
    let hex = color.strip_prefix('#').unwrap_or_default();
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(color)
    } else {
        Err(RuleError::Color(raw.to_owned()))
    }
}

/// Nombre de agente: 1–40 caracteres `a-z 0-9 . _ -`, empezando por letra o número.
pub fn agent_name(raw: &str) -> Result<String, RuleError> {
    let name = raw.trim().to_lowercase();
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if first_ok && rest_ok && name.len() <= 40 {
        Ok(name)
    } else {
        Err(RuleError::AgentName(raw.to_owned()))
    }
}

/// Nombre de interfaz (D22): 1–60 caracteres, letras (también con acentos),
/// números, espacios, punto, guion y guion bajo, empezando por letra o
/// número. Se guarda tal como se escribió («Claude Code»).
pub fn interface_name(raw: &str) -> Result<String, RuleError> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(char::is_alphanumeric);
    let rest_ok = chars.all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'));
    if first_ok && rest_ok && name.chars().count() <= 60 {
        Ok(name)
    } else {
        Err(RuleError::InterfaceName(raw.to_owned()))
    }
}

/// Cuerpo de comentario en Markdown: no vacío, hasta 10 000 caracteres.
pub fn comment_body(raw: &str) -> Result<String, RuleError> {
    let body = raw.trim();
    if body.is_empty() || body.chars().count() > 10_000 {
        Err(RuleError::CommentBody)
    } else {
        Ok(body.to_owned())
    }
}

/// Color estable derivado del nombre, para etiquetas creadas sin color.
pub fn default_label_color(name: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#e5484d", "#f76b15", "#ffc53d", "#46a758", "#12a594", "#0090ff", "#6e56cf", "#d6409f",
    ];
    let hash = name
        .to_lowercase()
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let idx = usize::try_from(hash).unwrap_or_default() % PALETTE.len();
    PALETTE.get(idx).copied().unwrap_or("#0090ff").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_keys() {
        assert_eq!(project_key(" api ").unwrap(), "API");
        assert_eq!(project_key("web2").unwrap(), "WEB2");
        for bad in ["A", "2FA", "MUY-LARGA", "ÑU", "ABCDEFGHIJK"] {
            assert!(project_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn colors_and_labels() {
        assert_eq!(label_color("#FFAA00").unwrap(), "#ffaa00");
        assert!(label_color("ffaa00").is_err());
        assert!(label_color("#ffaa0").is_err());
        assert!(label_name("a,b").is_err());
        assert_eq!(default_label_color("Rust"), default_label_color("rust"));
    }

    #[test]
    fn agent_names_and_comments() {
        assert_eq!(agent_name(" Claude-1 ").unwrap(), "claude-1");
        assert_eq!(agent_name("cursor.bot_2").unwrap(), "cursor.bot_2");
        for bad in ["", "-x", "con espacio", "ñandú", &"a".repeat(41)] {
            assert!(agent_name(bad).is_err(), "{bad}");
        }
        assert!(comment_body("  ").is_err());
        assert_eq!(comment_body(" **ok** ").unwrap(), "**ok**");
    }

    #[test]
    fn interface_names_keep_case_and_spaces() {
        assert_eq!(interface_name("  Claude   Code ").unwrap(), "Claude Code");
        assert_eq!(interface_name("Cursor (Ñandú)").ok(), None);
        assert_eq!(interface_name("Ñandú 2.0").unwrap(), "Ñandú 2.0");
        for bad in ["", "  ", "-x", "a/b", &"a".repeat(61)] {
            assert!(interface_name(bad).is_err(), "{bad}");
        }
    }
}
