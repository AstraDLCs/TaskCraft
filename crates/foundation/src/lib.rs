//! L0 — Fundamentos: tipos básicos, matemáticas y utilidades comunes.
//!
//! No depende de ningún crate interno.

mod id;
pub mod secret;

pub use id::CorrelationId;
#[doc(hidden)]
pub use serde;
pub use ulid::{DecodeError, Ulid};

/// ULID nuevo, siempre mayor que el anterior de este proceso aunque caigan
/// en el mismo milisegundo: el orden de los IDs es el orden de creación
/// (historial, cursores, reproducción de métricas).
pub fn next_ulid() -> Ulid {
    use std::sync::{Mutex, OnceLock, PoisonError};
    static GENERATOR: OnceLock<Mutex<ulid::Generator>> = OnceLock::new();
    let generator = GENERATOR.get_or_init(|| Mutex::new(ulid::Generator::new()));
    let mut g = generator.lock().unwrap_or_else(PoisonError::into_inner);
    // Desbordar 80 bits aleatorios en un milisegundo no ocurre en la práctica;
    // si ocurriera, un ULID nuevo sigue siendo único.
    g.generate().unwrap_or_else(|_| Ulid::generate())
}
