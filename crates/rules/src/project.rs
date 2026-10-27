//! Reglas de lo que cada proyecto define: flujo y etapas (D10, RF-MOD-07),
//! estimación (RF-MOD-14), campos personalizados (RF-MOD-15), menciones
//! `[[API-42]]` (RF-REL-04) y citas de diagramas `[[API-D3]]` (RF-MOD-19).

use model::{
    AgentId, DiagramKey, Estimate, EstimateUnit, FieldDef, FieldKind, Project, Status, StatusDef,
    TaskKey,
};

use crate::RuleError;

/// Columnas del flujo del proyecto, en orden. Los estados base que falten
/// (flujo vacío o incompleto) se añaden en su orden por defecto.
pub fn workflow(project: &Project) -> Vec<StatusDef> {
    let order = |s: Status| Status::ALL.iter().position(|b| *b == s).unwrap_or(0);
    let mut out = project.workflow.clone();
    for base in Status::ALL {
        if !out.iter().any(|d| d.key == base.as_str()) {
            let at = out
                .iter()
                .position(|d| order(d.base) >= order(*base))
                .unwrap_or(out.len());
            out.insert(
                at,
                StatusDef::new(base.as_str().to_owned(), String::new(), *base),
            );
        }
    }
    out
}

/// Clave de estado del proyecto (base, alias en español o etapa) → estado
/// base y etapa.
pub fn resolve_status(project: &Project, key: &str) -> Result<(Status, Option<String>), RuleError> {
    if let Ok(base) = key.parse::<Status>() {
        return Ok((base, None));
    }
    let wanted = slug(key);
    let flow = workflow(project);
    match flow
        .iter()
        .find(|d| !d.is_base() && (d.key == wanted || slug(&d.name) == wanted))
    {
        Some(d) => Ok((d.base, Some(d.key.clone()))),
        None => Err(RuleError::UnknownStatus {
            key: key.to_owned(),
            valid: flow
                .iter()
                .map(|d| d.key.clone())
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

/// Agente dueño de la columna en la que queda una tarea (D20): su etapa o,
/// sin etapa, su estado base.
pub fn column_owner(project: &Project, status: Status, stage: Option<&str>) -> Option<AgentId> {
    let key = stage.unwrap_or(status.as_str());
    project
        .workflow
        .iter()
        .find(|d| d.key == key)
        .and_then(|d| d.owner)
}

/// `Pruebas QA` → `pruebas_qa`: minúsculas ASCII, dígitos y `_`.
pub fn slug(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// Clave de una etapa nueva: válida y sin chocar con un estado base ni con
/// otra columna del flujo.
pub fn stage_key(project: &Project, raw: &str) -> Result<String, RuleError> {
    let key = slug(raw);
    if key.is_empty() || key.len() > 40 {
        return Err(RuleError::StageKey(raw.to_owned()));
    }
    let taken = key.parse::<Status>().is_ok() || workflow(project).iter().any(|d| d.key == key);
    if taken {
        return Err(RuleError::StageExists(key));
    }
    Ok(key)
}

/// `1.5` (horas o puntos) o `M` (tallas) → estimación en centésimas.
pub fn parse_estimate(unit: EstimateUnit, raw: &str) -> Result<Estimate, RuleError> {
    let text = raw.trim().replace(',', ".");
    let bad = || RuleError::Estimate {
        value: raw.to_owned(),
        unit: unit.as_str().to_owned(),
    };
    match unit {
        EstimateUnit::Size => Estimate::SIZES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&text))
            .map(|(_, v)| Estimate(*v))
            .ok_or_else(bad),
        EstimateUnit::Hours | EstimateUnit::Points => {
            let value: f64 = text.parse().map_err(|_| bad())?;
            if !(0.0..=100_000.0).contains(&value) {
                return Err(bad());
            }
            Ok(Estimate((value * 100.0).round() as u32))
        }
    }
}

/// Texto de una estimación en la unidad del proyecto: `1.5`, `8`, `M`.
pub fn format_estimate(unit: EstimateUnit, e: Estimate) -> String {
    match unit {
        EstimateUnit::Size => Estimate::SIZES
            .iter()
            .find(|(_, v)| *v == e.0)
            .map(|(n, _)| (*n).to_owned())
            .unwrap_or_else(|| format!("{}", e.as_f64())),
        _ => {
            let v = e.as_f64();
            if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                format!("{v}")
            }
        }
    }
}

/// Definición de un campo personalizado nuevo, validada.
pub fn field_def(
    project: &Project,
    name: &str,
    kind: FieldKind,
    options: Vec<String>,
) -> Result<FieldDef, RuleError> {
    let key = slug(name);
    if key.is_empty() || name.trim().chars().count() > 60 {
        return Err(RuleError::FieldKey(name.to_owned()));
    }
    if project.fields.iter().any(|f| f.key == key) {
        return Err(RuleError::FieldExists(key));
    }
    Ok(FieldDef {
        key,
        name: name.trim().to_owned(),
        kind,
        options: field_options(kind, options)?,
    })
}

/// Opciones de un campo de selección: no vacías, sin comas ni repetidas.
pub fn field_options(kind: FieldKind, options: Vec<String>) -> Result<Vec<String>, RuleError> {
    let needs = matches!(kind, FieldKind::Select | FieldKind::Multiselect);
    let mut out: Vec<String> = Vec::new();
    for o in options {
        let o = o.trim().to_owned();
        if o.is_empty() || o.contains(',') || o.chars().count() > 60 {
            return Err(RuleError::FieldOptions);
        }
        if !out.contains(&o) {
            out.push(o);
        }
    }
    if needs == out.is_empty() {
        return Err(RuleError::FieldOptions);
    }
    Ok(out)
}

/// Campo por clave o nombre.
pub fn find_field<'p>(project: &'p Project, key: &str) -> Result<&'p FieldDef, RuleError> {
    let wanted = slug(key);
    project
        .fields
        .iter()
        .find(|f| f.key == wanted)
        .ok_or_else(|| RuleError::UnknownField {
            key: key.to_owned(),
            valid: project
                .fields
                .iter()
                .map(|f| f.key.clone())
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// Valor de un campo como texto canónico, según su tipo.
pub fn field_value(def: &FieldDef, raw: &str) -> Result<String, RuleError> {
    let text = raw.trim();
    let bad = || RuleError::FieldValue {
        field: def.key.clone(),
        kind: def.kind.as_str().to_owned(),
        value: raw.to_owned(),
    };
    match def.kind {
        FieldKind::Text => {
            if text.chars().count() > 1000 {
                return Err(bad());
            }
            Ok(text.to_owned())
        }
        FieldKind::Number => {
            let v: f64 = text.replace(',', ".").parse().map_err(|_| bad())?;
            if !v.is_finite() {
                return Err(bad());
            }
            Ok(format!("{v}"))
        }
        FieldKind::Bool => match text.to_lowercase().as_str() {
            "true" | "si" | "sí" | "yes" | "1" => Ok("true".into()),
            "false" | "no" | "0" => Ok("false".into()),
            _ => Err(bad()),
        },
        FieldKind::Date => text
            .parse::<jiff::civil::Date>()
            .map(|d| d.to_string())
            .map_err(|_| bad()),
        FieldKind::Url => {
            let ok = (text.starts_with("http://") || text.starts_with("https://"))
                && text.len() > 8
                && !text.contains(char::is_whitespace);
            if ok { Ok(text.to_owned()) } else { Err(bad()) }
        }
        FieldKind::Select => def
            .options
            .iter()
            .find(|o| o.eq_ignore_ascii_case(text))
            .cloned()
            .ok_or_else(bad),
        FieldKind::Multiselect => {
            let mut chosen = Vec::new();
            for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let o = def
                    .options
                    .iter()
                    .find(|o| o.eq_ignore_ascii_case(part))
                    .ok_or_else(bad)?;
                if !chosen.contains(o) {
                    chosen.push(o.clone());
                }
            }
            // En el orden de las opciones, para que el texto sea canónico.
            chosen.sort_by_key(|c| def.options.iter().position(|o| o == c));
            Ok(chosen.join(","))
        }
    }
}

/// Claves mencionadas con `[[API-42]]` en un texto, sin repetir.
pub fn mentions(text: &str) -> Vec<TaskKey> {
    bracketed(text, TaskKey::parse)
}

/// Diagramas citados con `[[API-D3]]` o `![[API-D3]]` en un texto, sin repetir.
pub fn diagram_mentions(text: &str) -> Vec<DiagramKey> {
    bracketed(text, DiagramKey::parse)
}

fn bracketed<K: PartialEq>(text: &str, parse: fn(&str) -> Option<K>) -> Vec<K> {
    let mut out: Vec<K> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else { break };
        if let Some(key) = parse(rest[..end].trim())
            && !out.contains(&key)
        {
            out.push(key);
        }
        rest = &rest[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{ProjectId, ProjectStatus, Timestamp};

    fn project() -> Project {
        Project {
            id: ProjectId::generate(),
            key: "API".into(),
            name: "API".into(),
            description: String::new(),
            status: ProjectStatus::Active,
            next_number: 1,
            next_diagram: 1,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            locked: false,
            workflow: Vec::new(),
            estimate_unit: EstimateUnit::Hours,
            fields: Vec::new(),
        }
    }

    #[test]
    fn default_workflow_is_the_six_bases_and_stages_slot_in() {
        let mut p = project();
        let keys: Vec<String> = workflow(&p).into_iter().map(|d| d.key).collect();
        assert_eq!(
            keys,
            [
                "backlog",
                "todo",
                "in_progress",
                "in_review",
                "done",
                "cancelled"
            ]
        );

        let qa = stage_key(&p, "Pruebas QA").unwrap();
        assert_eq!(qa, "pruebas_qa");
        p.workflow = workflow(&p);
        p.workflow.insert(
            4,
            StatusDef::new(qa.clone(), "Pruebas QA", Status::InReview),
        );
        assert_eq!(
            resolve_status(&p, "pruebas qa").unwrap(),
            (Status::InReview, Some(qa.clone()))
        );
        assert_eq!(
            resolve_status(&p, "en_progreso").unwrap(),
            (Status::InProgress, None)
        );
        assert!(matches!(
            stage_key(&p, "todo"),
            Err(RuleError::StageExists(_))
        ));
        assert!(matches!(
            stage_key(&p, "pruebas-qa"),
            Err(RuleError::StageExists(_))
        ));
        let err = resolve_status(&p, "nada").unwrap_err();
        assert!(
            matches!(err, RuleError::UnknownStatus { valid, .. } if valid.contains("pruebas_qa"))
        );
    }

    #[test]
    fn estimates_in_each_unit() {
        let h = parse_estimate(EstimateUnit::Hours, "1,5").unwrap();
        assert_eq!(h, Estimate(150));
        assert_eq!(format_estimate(EstimateUnit::Hours, h), "1.5");
        assert_eq!(format_estimate(EstimateUnit::Points, Estimate(800)), "8");
        let m = parse_estimate(EstimateUnit::Size, "m").unwrap();
        assert_eq!(format_estimate(EstimateUnit::Size, m), "M");
        assert!(parse_estimate(EstimateUnit::Size, "XXL").is_err());
        assert!(parse_estimate(EstimateUnit::Hours, "-1").is_err());
    }

    #[test]
    fn field_values_are_validated_and_canonical() {
        let p = project();
        let multi = field_def(
            &p,
            "Plataformas",
            FieldKind::Multiselect,
            vec!["Linux".into(), "Windows".into(), "Mac".into()],
        )
        .unwrap();
        assert_eq!(field_value(&multi, "mac, linux").unwrap(), "Linux,Mac");
        assert!(field_value(&multi, "beos").is_err());
        let date = field_def(&p, "Entrega", FieldKind::Date, vec![]).unwrap();
        assert_eq!(field_value(&date, "2026-11-01").unwrap(), "2026-11-01");
        assert!(field_value(&date, "mañana").is_err());
        let url = field_def(&p, "Diseño", FieldKind::Url, vec![]).unwrap();
        assert!(field_value(&url, "ftp://x").is_err());
        assert!(field_def(&p, "Sin opciones", FieldKind::Select, vec![]).is_err());
        let b = field_def(&p, "Urgente", FieldKind::Bool, vec![]).unwrap();
        assert_eq!(field_value(&b, "sí").unwrap(), "true");
    }

    #[test]
    fn mentions_are_found_once_each() {
        let found = mentions("Ver [[API-42]] y [[web-3]], otra vez [[API-42]]; [[nada]] [[");
        let keys: Vec<String> = found.iter().map(ToString::to_string).collect();
        assert_eq!(keys, ["API-42", "WEB-3"]);
    }

    #[test]
    fn diagram_mentions_are_found_once_each() {
        let text = "Ver [[api-d3]] y [[API-42]]\n![[API-D3]]\n![[WEB-D1]] [[API-D]]";
        let keys: Vec<String> = diagram_mentions(text)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(keys, ["API-D3", "WEB-D1"]);
        assert_eq!(mentions(text).len(), 1);
    }
}
