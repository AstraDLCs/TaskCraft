//! Flujo y datos propios de cada proyecto: estados (D10, RF-MOD-07),
//! estimación (RF-MOD-14) y campos personalizados (RF-MOD-15).

use serde::{Deserialize, Serialize};

use crate::Status;
use crate::text_enum::text_enum;

/// Una columna del flujo de un proyecto (D10). Si `key` es un estado base,
/// lo renombra (nombre vacío = el traducido); si no, es una etapa propia
/// que cuelga de `base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDef {
    pub key: String,
    #[serde(default)]
    pub name: String,
    pub base: Status,
    /// Límite de tareas en la columna (RF-COO-08).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip_limit: Option<u32>,
    /// Con el límite lleno: rechazar la entrada (si no, solo se avisa).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wip_reject: bool,
    /// Agente al que pertenece la columna (su cola o su mesa de trabajo, D20).
    /// Lo que entra en ella pasa a estar asignado a él.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<crate::AgentId>,
}

impl StatusDef {
    pub fn new(key: impl Into<String>, name: impl Into<String>, base: Status) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            base,
            wip_limit: None,
            wip_reject: false,
            owner: None,
        }
    }

    /// ¿Es uno de los 6 estados base (y no una etapa)?
    pub fn is_base(&self) -> bool {
        self.key == self.base.as_str()
    }
}

text_enum! {
    /// Unidad de estimación de un proyecto (RF-MOD-14).
    pub enum EstimateUnit ("error-parse-estimate-unit", "estimate-unit") {
        Hours = "hours" | "horas",
        Points = "points" | "puntos",
        Size = "size" | "talla",
    }
}

/// Esfuerzo estimado en centésimas de la unidad del proyecto (1,5 h = 150).
/// En tallas: S = 100, M = 200, L = 300, XL = 500.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Estimate(pub u32);

impl Estimate {
    pub const SIZES: [(&str, u32); 4] = [("S", 100), ("M", 200), ("L", 300), ("XL", 500)];

    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}

text_enum! {
    /// Tipo de un campo personalizado (RF-MOD-15).
    pub enum FieldKind ("error-parse-field-kind", "field-kind") {
        Text = "text" | "texto",
        Number = "number" | "numero" | "número",
        Select = "select" | "seleccion" | "selección",
        Multiselect = "multiselect" | "multiseleccion" | "multiselección",
        Bool = "bool" | "boolean" | "booleano",
        Date = "date" | "fecha",
        Url = "url",
    }
}

/// Definición de un campo personalizado de un proyecto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDef {
    /// Clave estable en minúsculas (`cliente`, `version_objetivo`).
    pub key: String,
    pub name: String,
    pub kind: FieldKind,
    /// Opciones de `select` y `multiselect`.
    #[serde(default)]
    pub options: Vec<String>,
}
