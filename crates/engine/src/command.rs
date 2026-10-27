use std::collections::BTreeMap;

use model::Timestamp;
use serde_json::Value;
use store::Writer;

use crate::{Args, Context, EngineError};

/// Lo que recibe un comando al ejecutarse: la transacción abierta, quién lo
/// pide y la hora del comando (la misma para todos sus cambios).
pub struct Cx<'a> {
    pub tx: &'a mut dyn Writer,
    pub ctx: &'a Context,
    pub now: Timestamp,
    /// Historial ya formado que aporta un comando [`Command::raw`] (la
    /// importación); el resto lo deja vacío y el engine lo calcula.
    pub history: Vec<model::ActivityEntry>,
    /// Carpeta del workspace, si la hay (copias de seguridad, RF-ALM-06).
    pub home: Option<&'a std::path::Path>,
}

/// Tipo de valor que espera un parámetro. Todos llegan como texto (`Args`);
/// el tipo sirve para validar, generar JSON Schema (MCP) y formularios (UI, CLI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Int,
    Bool,
    /// `2026-11-01`, `2026-11-01T17:00` o con zona `[America/Lima]`.
    Date,
    /// Lista separada por comas.
    List,
    /// Documento JSON (planificación en lote).
    Json,
    /// Uno de estos valores canónicos.
    Enum(&'static [&'static str]),
}

/// Parámetro declarado de un comando. La ayuda se traduce con `param-<name>`.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
}

impl Param {
    pub fn help_key(&self) -> String {
        format!("param-{}", self.name)
    }
}

/// Una acción del programa, identificada por un ID estable `dominio.accion`.
pub trait Command: Send + Sync {
    fn id(&self) -> &'static str;

    /// Descripción para menús, ayuda y tools MCP.
    fn label(&self) -> &'static str;

    /// Parámetros que acepta; el engine rechaza los que no estén aquí.
    fn params(&self) -> &'static [Param] {
        &[]
    }

    /// `false` si solo lee (MCP lo anuncia como `readOnlyHint`).
    fn mutates(&self) -> bool {
        true
    }

    /// `true` si escribe sin pasar por el `Recorder` y aporta su propio
    /// historial en `Cx::history`. Solo para la importación (RF-EXP-03a), que
    /// trae el historial original y no debe registrarse como cambios nuevos.
    fn raw(&self) -> bool {
        false
    }

    /// Devuelve el resultado en JSON (lo que verá el cliente).
    fn run(&self, cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError>;
}

#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<&'static str, Box<dyn Command>>,
}

impl Registry {
    /// Los comandos incluidos de serie. Sirve también a quien solo necesita
    /// la metadata (MCP, CLI, paleta) sin un almacenamiento abierto.
    pub fn standard() -> Self {
        let mut r = Self::default();
        crate::commands::register(&mut r);
        r
    }

    pub fn register(&mut self, command: impl Command + 'static) {
        self.commands.insert(command.id(), Box::new(command));
    }

    pub fn get(&self, id: &str) -> Option<&dyn Command> {
        self.commands.get(id).map(|c| c.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Command> {
        self.commands.values().map(|c| c.as_ref())
    }
}

/// Declara un comando sin estado a partir de una función:
///
/// `command!(Get, "task.get", "cmd-task-get", get, read, [req "task": K::Text]);`
///
/// `read`/`write` indica si modifica datos (`raw`: escribe sin `Recorder`,
/// ver [`Command::raw`]); cada parámetro es `req` (obligatorio) u `opt`.
macro_rules! command {
    (
        $ty:ident, $id:literal, $label:literal, $run:path, $mode:ident,
        [ $( $req:ident $name:literal : $kind:expr ),* $(,)? ]
    ) => {
        pub(crate) struct $ty;

        impl $crate::Command for $ty {
            fn id(&self) -> &'static str {
                $id
            }

            fn label(&self) -> &'static str {
                $label
            }

            fn params(&self) -> &'static [$crate::Param] {
                const PARAMS: &[$crate::Param] = &[$(
                    $crate::Param {
                        name: $name,
                        kind: $kind,
                        required: $crate::command::required!($req),
                    }
                ),*];
                PARAMS
            }

            fn mutates(&self) -> bool {
                $crate::command::mutates!($mode)
            }

            fn raw(&self) -> bool {
                $crate::command::raw!($mode)
            }

            fn run(
                &self,
                cx: &mut $crate::Cx<'_>,
                args: &$crate::Args,
            ) -> Result<serde_json::Value, $crate::EngineError> {
                $run(cx, args)
            }
        }
    };
}

macro_rules! required {
    (req) => {
        true
    };
    (opt) => {
        false
    };
}

macro_rules! mutates {
    (read) => {
        false
    };
    (write) => {
        true
    };
    (raw) => {
        true
    };
}

macro_rules! raw {
    (raw) => {
        true
    };
    (read) => {
        false
    };
    (write) => {
        false
    };
}

pub(crate) use {command, mutates, raw, required};
