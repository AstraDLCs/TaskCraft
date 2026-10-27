use std::collections::BTreeMap;
use std::str::FromStr;

use i18n::Msg;
use model::When;

use crate::EngineError;
use crate::error::ArgError;

/// Argumentos de un comando: pares clave/valor en texto, para que botones,
/// CLI y MCP los construyan igual.
#[derive(Debug, Clone, Default)]
pub struct Args(BTreeMap<String, String>);

impl Args {
    pub fn none() -> Self {
        Self::default()
    }

    /// Desde los argumentos de una petición del protocolo.
    pub fn from_map(map: protocol::ArgMap) -> Self {
        Self(map)
    }

    pub fn to_map(&self) -> protocol::ArgMap {
        self.0.clone()
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// Sin `key` (lo usa el engine para `as_agent`, D15).
    pub fn without(&self, key: &str) -> Self {
        let mut map = self.0.clone();
        map.remove(key);
        Self(map)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn require(&self, key: &str) -> Result<&str, EngineError> {
        self.get(key)
            .ok_or_else(|| EngineError::MissingArg(key.to_owned()))
    }

    pub fn parse<T>(&self, key: &str) -> Result<Option<T>, EngineError>
    where
        T: FromStr,
        T::Err: ArgError,
    {
        self.get(key)
            .map(|v| {
                v.parse()
                    .map_err(|e: T::Err| EngineError::arg(key, e.reason(v)))
            })
            .transpose()
    }

    pub fn parse_required<T>(&self, key: &str) -> Result<T, EngineError>
    where
        T: FromStr,
        T::Err: ArgError,
    {
        self.parse(key)?
            .ok_or_else(|| EngineError::MissingArg(key.to_owned()))
    }

    /// `true`/`false`, `si`/`no`, `1`/`0`. Ausente = `false`.
    pub fn flag(&self, key: &str) -> Result<bool, EngineError> {
        match self.get(key).map(str::to_lowercase).as_deref() {
            None => Ok(false),
            Some("true" | "si" | "sí" | "yes" | "1") => Ok(true),
            Some("false" | "no" | "0") => Ok(false),
            Some(other) => Err(EngineError::arg(
                key,
                Msg::new("arg-not-bool").arg("value", other),
            )),
        }
    }

    /// Lista separada por comas, sin vacíos.
    pub fn list(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Campo opcional que se puede borrar: ausente = no tocar,
    /// vacío o `none` = borrar, otro valor = asignar.
    pub fn clearable<T>(
        &self,
        key: &str,
        parse: impl FnOnce(&str) -> Result<T, EngineError>,
    ) -> Result<Option<Option<T>>, EngineError> {
        match self.get(key) {
            None => Ok(None),
            Some(v) if v.is_empty() || v.eq_ignore_ascii_case("none") => Ok(Some(None)),
            Some(v) => parse(v).map(|t| Some(Some(t))),
        }
    }
}

/// `2026-11-01`, `2026-11-01T17:00` (zona del sistema) o con zona explícita
/// `2026-11-01T17:00-05:00[America/Lima]`.
pub(crate) fn parse_when(key: &str, text: &str) -> Result<When, EngineError> {
    if let Ok(when) = text.parse() {
        return Ok(when);
    }
    let bad = |e: jiff::Error| EngineError::arg(key, e.reason(text));
    let local: jiff::civil::DateTime = text.parse().map_err(bad)?;
    local
        .to_zoned(jiff::tz::TimeZone::system())
        .map(When::At)
        .map_err(bad)
}

/// Rechaza argumentos que el comando no declara y exige los obligatorios, para
/// que un agente sepa al instante qué escribió mal.
pub(crate) fn validate(params: &[crate::Param], args: &Args) -> Result<(), EngineError> {
    for key in args.keys() {
        if !params.iter().any(|p| p.name == key) {
            let valid: Vec<&str> = params.iter().map(|p| p.name).collect();
            return Err(EngineError::arg(
                key,
                Msg::new("arg-unknown").arg("valid", valid.join(", ")),
            ));
        }
    }
    for p in params.iter().filter(|p| p.required) {
        if args.get(p.name).is_none() {
            return Err(EngineError::MissingArg(p.name.to_owned()));
        }
    }
    Ok(())
}
