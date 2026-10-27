//! L0 — Idiomas (RNF-12): catálogos Fluent embebidos para español e inglés.
//!
//! El código nunca escribe textos para personas: construye un [`Msg`] (clave +
//! argumentos) y el frontend lo traduce con [`tr`] al idioma de quien lo lee.

use std::borrow::Cow;
use std::fmt;
use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};

/// Variable de entorno que fuerza el idioma (`es`, `en`).
pub const LANG_ENV: &str = "TASKCRAFT_LANG";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    Es,
    /// Por defecto, y siempre para agentes salvo que se fuerce otro.
    #[default]
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Es, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::En => "en",
        }
    }

    /// Nombre del idioma en ese mismo idioma, para el selector.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::Es => "Español",
            Self::En => "English",
        }
    }

    /// `es`, `es-PE`, `es_PE.UTF-8` → `Es`. Idiomas no soportados → `None`.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_lowercase();
        match primary.as_str() {
            "es" => Some(Self::Es),
            "en" => Some(Self::En),
            _ => None,
        }
    }

    /// `TASKCRAFT_LANG`, y si no, el idioma del sistema (`LC_ALL`, `LC_MESSAGES`,
    /// `LANG`). Si nada coincide, inglés.
    pub fn detect() -> Self {
        [LANG_ENV, "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|var| std::env::var(var).ok())
            .find_map(|value| Self::from_tag(&value))
            .unwrap_or_default()
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::str::FromStr for Lang {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_tag(s).ok_or_else(|| format!("unsupported language `{s}`; use es or en"))
    }
}

/// Valor de un argumento: los números permiten plurales en el catálogo.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Text(String),
    Number(i64),
}

impl From<&str> for Arg {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}

impl From<String> for Arg {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<&String> for Arg {
    fn from(v: &String) -> Self {
        Self::Text(v.clone())
    }
}

macro_rules! number_arg {
    ($($t:ty),+) => {$(
        impl From<$t> for Arg {
            fn from(v: $t) -> Self {
                Self::Number(i64::try_from(v).unwrap_or(i64::MAX))
            }
        }
    )+};
}

number_arg!(i64, u64, u32, usize);

/// Texto pendiente de traducir: clave del catálogo + argumentos.
#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub key: &'static str,
    pub args: Vec<(&'static str, Arg)>,
}

impl Msg {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, name: &'static str, value: impl Into<Arg>) -> Self {
        self.args.push((name, value.into()));
        self
    }

    /// Valor de un argumento, para tests y para quien necesite el dato crudo.
    pub fn get(&self, name: &str) -> Option<&Arg> {
        self.args.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }
}

/// Inglés, para logs y `Display`.
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tr(Lang::En, self))
    }
}

/// Lo que puede explicarse a una persona en su idioma.
pub trait Localize {
    fn msg(&self) -> Msg;

    fn localize(&self, lang: Lang) -> String {
        tr(lang, &self.msg())
    }
}

impl Localize for Msg {
    fn msg(&self) -> Msg {
        self.clone()
    }
}

const ES: &str = include_str!("../locales/es/taskcraft.ftl");
const EN: &str = include_str!("../locales/en/taskcraft.ftl");

fn source(lang: Lang) -> &'static str {
    match lang {
        Lang::Es => ES,
        Lang::En => EN,
    }
}

fn bundle(lang: Lang) -> &'static FluentBundle<FluentResource> {
    static ES_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
    static EN_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
    let cell = match lang {
        Lang::Es => &ES_BUNDLE,
        Lang::En => &EN_BUNDLE,
    };
    cell.get_or_init(|| build(lang))
}

fn build(lang: Lang) -> FluentBundle<FluentResource> {
    let locale = lang.code().parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![locale]);
    // Sin marcas de aislamiento Unicode: ensucian la salida de terminal.
    bundle.set_use_isolating(false);
    // Un catálogo con errores de sintaxis carga lo que sí parsea; los tests
    // garantizan que no ocurra.
    let resource = match FluentResource::try_new(source(lang).to_owned()) {
        Ok(r) => r,
        Err((r, _)) => r,
    };
    let _ = bundle.add_resource(resource);
    bundle
}

/// Traduce un mensaje. Si la clave no existe devuelve la clave: nunca falla.
pub fn tr(lang: Lang, msg: &Msg) -> String {
    let bundle = bundle(lang);
    let Some(pattern) = bundle.get_message(msg.key).and_then(|m| m.value()) else {
        return msg.key.to_owned();
    };
    let mut args = FluentArgs::new();
    for (name, value) in &msg.args {
        let value: FluentValue<'_> = match value {
            Arg::Text(t) => FluentValue::from(Cow::Borrowed(t.as_str())),
            Arg::Number(n) => FluentValue::from(*n),
        };
        args.set(*name, value);
    }
    let mut errors = Vec::new();
    bundle
        .format_pattern(pattern, Some(&args), &mut errors)
        .into_owned()
}

/// Traduce una clave sin argumentos.
pub fn t(lang: Lang, key: &'static str) -> String {
    tr(lang, &Msg::new(key))
}

/// Traducción de una clave construida en tiempo de ejecución (sin argumentos).
/// `None` si no existe, para que quien llama elija un respaldo.
pub fn lookup(lang: Lang, key: &str) -> Option<String> {
    let bundle = bundle(lang);
    let pattern = bundle.get_message(key)?.value()?;
    let mut errors = Vec::new();
    Some(
        bundle
            .format_pattern(pattern, None, &mut errors)
            .into_owned(),
    )
}

/// ¿Existe la clave en el catálogo de `lang`?
pub fn has(lang: Lang, key: &str) -> bool {
    bundle(lang).has_message(key)
}

#[cfg(test)]
mod tests;
