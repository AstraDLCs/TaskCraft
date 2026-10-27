use std::collections::{BTreeMap, BTreeSet};

use fluent_bundle::FluentResource;

use super::*;

/// Mensajes del catálogo → variables que usa cada uno (`$name`).
fn messages(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in src.lines() {
        let starts_message = line.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        if starts_message && let Some((id, _)) = line.split_once(" =") {
            current = Some(id.trim().to_owned());
            out.entry(id.trim().to_owned()).or_default();
        } else if !line.starts_with(' ') {
            current = None;
        }
        if let Some(id) = &current {
            let vars = out.entry(id.clone()).or_default();
            let mut rest = line;
            while let Some(i) = rest.find('$') {
                rest = &rest[i + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                vars.insert(name);
            }
        }
    }
    out
}

#[test]
fn catalogs_parse_without_errors() {
    for lang in Lang::ALL {
        if let Err((_, errors)) = FluentResource::try_new(source(lang).to_owned()) {
            panic!("{lang}: {errors:?}");
        }
    }
}

#[test]
fn spanish_and_english_have_the_same_keys_and_variables() {
    let es = messages(ES);
    let en = messages(EN);
    let es_keys: BTreeSet<_> = es.keys().collect();
    let en_keys: BTreeSet<_> = en.keys().collect();
    assert_eq!(
        es_keys.difference(&en_keys).collect::<Vec<_>>(),
        Vec::<&&String>::new(),
        "faltan en inglés"
    );
    assert_eq!(
        en_keys.difference(&es_keys).collect::<Vec<_>>(),
        Vec::<&&String>::new(),
        "faltan en español"
    );
    for (key, vars) in &es {
        assert_eq!(Some(vars), en.get(key), "variables distintas en `{key}`");
    }
    assert!(
        es.len() > 100,
        "el parser de test encontró {} mensajes",
        es.len()
    );
}

#[test]
fn translates_with_arguments_and_plurals() {
    let msg = Msg::new("notfound-task").arg("task", "API-9");
    assert_eq!(tr(Lang::Es, &msg), "No existe la tarea `API-9`");
    assert_eq!(tr(Lang::En, &msg), "Task `API-9` does not exist");
    assert_eq!(msg.to_string(), tr(Lang::En, &msg), "Display = inglés");

    let one = Msg::new("ui-activity-count").arg("count", 1u32);
    let many = Msg::new("ui-activity-count").arg("count", 5u32);
    assert_eq!(tr(Lang::Es, &one), "1 cambio");
    assert_eq!(tr(Lang::Es, &many), "5 cambios");
    assert_eq!(tr(Lang::En, &many), "5 changes");
}

#[test]
fn missing_key_falls_back_to_the_key() {
    assert_eq!(t(Lang::Es, "no-existe-esta-clave"), "no-existe-esta-clave");
    assert!(!has(Lang::En, "no-existe-esta-clave"));
    assert!(has(Lang::En, "ui-ready"));
}

#[test]
fn language_tags() {
    assert_eq!(Lang::from_tag("es_PE.UTF-8"), Some(Lang::Es));
    assert_eq!(Lang::from_tag("en-US"), Some(Lang::En));
    assert_eq!(Lang::from_tag("ES"), Some(Lang::Es));
    assert_eq!(Lang::from_tag("fr_FR"), None);
    assert_eq!(Lang::from_tag("C"), None);
    assert_eq!("es".parse::<Lang>(), Ok(Lang::Es));
    assert!("de".parse::<Lang>().is_err());
}
