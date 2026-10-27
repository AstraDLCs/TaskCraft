//! Adaptador mínimo sobre `postgres` para que el SQL y la lectura de filas
//! se parezcan a los de `store-sqlite`: parámetros numerados `?N`, enteros y
//! banderas como `BIGINT` (0/1) y lectura tipada de columnas.

use std::error::Error as StdError;

use bytes::BytesMut;
use postgres::types::{IsNull, ToSql, Type, to_sql_checked};

pub(crate) type Result<T> = std::result::Result<T, postgres::Error>;

/// Valor de un parámetro. Se adapta al tipo que pide la columna.
#[derive(Debug, Clone)]
pub(crate) enum Param {
    Text(Option<String>),
    Int(Option<i64>),
    Bool(bool),
    Bytes(Vec<u8>),
}

type BoxError = Box<dyn StdError + Sync + Send>;

impl ToSql for Param {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> std::result::Result<IsNull, BoxError> {
        match self {
            Self::Text(v) => v.to_sql(ty, out),
            Self::Bytes(v) => v.to_sql(ty, out),
            Self::Int(None) => Ok(IsNull::Yes),
            Self::Int(Some(n)) => int(*n, ty, out),
            Self::Bool(b) if *ty == Type::BOOL => b.to_sql(ty, out),
            Self::Bool(b) => int(i64::from(*b), ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::NAME
                | Type::UNKNOWN
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::BOOL
                | Type::BYTEA
        )
    }

    to_sql_checked!();
}

fn int(n: i64, ty: &Type, out: &mut BytesMut) -> std::result::Result<IsNull, BoxError> {
    match *ty {
        Type::INT2 => i16::try_from(n)?.to_sql(ty, out),
        Type::INT4 => i32::try_from(n)?.to_sql(ty, out),
        Type::BOOL => (n != 0).to_sql(ty, out),
        _ => n.to_sql(ty, out),
    }
}

fn to_i64<T: TryInto<i64>>(n: T) -> Option<i64> {
    n.try_into().ok()
}

macro_rules! from_text {
    ($($t:ty => $f:expr),* $(,)?) => {
        $(impl From<&$t> for Param {
            fn from(v: &$t) -> Self {
                #[allow(clippy::redundant_closure_call)]
                Self::Text(($f)(v))
            }
        })*
    };
}

from_text! {
    String => |v: &String| Some(v.clone()),
    &str => |v: &&str| Some((*v).to_owned()),
    &String => |v: &&String| Some((*v).clone()),
    Option<String> => |v: &Option<String>| v.clone(),
    Option<&str> => |v: &Option<&str>| v.map(str::to_owned),
}

macro_rules! from_int {
    ($($t:ty),*) => {
        $(impl From<&$t> for Param {
            fn from(v: &$t) -> Self {
                Self::Int(to_i64(*v))
            }
        }
        impl From<&Option<$t>> for Param {
            fn from(v: &Option<$t>) -> Self {
                Self::Int(v.and_then(to_i64))
            }
        })*
    };
}

from_int!(u32, u64, i64, usize);

impl From<&bool> for Param {
    fn from(v: &bool) -> Self {
        Self::Bool(*v)
    }
}

impl From<&&[u8]> for Param {
    fn from(v: &&[u8]) -> Self {
        Self::Bytes(v.to_vec())
    }
}

/// `p![a, b]`: parámetros de una consulta.
macro_rules! p {
    ($($e:expr),* $(,)?) => {
        vec![$($crate::pg::Param::from(&$e)),*]
    };
}
pub(crate) use p;

/// `?N` (estilo SQLite) → `$N` (PostgreSQL).
pub(crate) fn sql(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '?' && chars.peek().is_some_and(char::is_ascii_digit) {
            out.push('$');
        } else {
            out.push(c);
        }
    }
    out
}

/// Referencias para la API de `postgres`.
pub(crate) fn refs(params: &[Param]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
}

/// Fila con lectura tipada por nombre o posición, como en `rusqlite`.
pub(crate) struct Row<'a>(pub &'a postgres::Row);

pub(crate) trait Index: Copy {
    fn position(self, row: &postgres::Row) -> usize;
}

impl Index for usize {
    fn position(self, _: &postgres::Row) -> usize {
        self
    }
}

impl Index for &str {
    fn position(self, row: &postgres::Row) -> usize {
        row.columns()
            .iter()
            .position(|c| c.name() == self)
            .unwrap_or(usize::MAX)
    }
}

pub(crate) trait Col: Sized {
    fn read(row: &postgres::Row, idx: usize) -> Result<Self>;
}

impl Row<'_> {
    pub fn get<T: Col>(&self, idx: impl Index) -> Result<T> {
        T::read(self.0, idx.position(self.0))
    }
}

fn int_col(row: &postgres::Row, idx: usize) -> Result<Option<i64>> {
    let ty = row.columns().get(idx).map(|c| c.type_().clone());
    match ty {
        Some(Type::INT4) => Ok(row.try_get::<_, Option<i32>>(idx)?.map(i64::from)),
        Some(Type::INT2) => Ok(row.try_get::<_, Option<i16>>(idx)?.map(i64::from)),
        Some(Type::BOOL) => Ok(row.try_get::<_, Option<bool>>(idx)?.map(i64::from)),
        _ => row.try_get::<_, Option<i64>>(idx),
    }
}

impl Col for String {
    fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
        row.try_get(idx)
    }
}

impl Col for Option<String> {
    fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
        row.try_get(idx)
    }
}

impl Col for Vec<u8> {
    fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
        row.try_get(idx)
    }
}

impl Col for bool {
    fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
        Ok(int_col(row, idx)?.is_some_and(|n| n != 0))
    }
}

macro_rules! int_cols {
    ($($t:ty),*) => {
        $(impl Col for $t {
            fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
                Ok(int_col(row, idx)?.and_then(|n| <$t>::try_from(n).ok()).unwrap_or_default())
            }
        }
        impl Col for Option<$t> {
            fn read(row: &postgres::Row, idx: usize) -> Result<Self> {
                Ok(int_col(row, idx)?.and_then(|n| <$t>::try_from(n).ok()))
            }
        })*
    };
}

int_cols!(u32, u64, i64, usize);

#[cfg(test)]
mod tests {
    #[test]
    fn placeholders_are_renumbered() {
        assert_eq!(
            super::sql("WHERE a = ?1 AND b IN (?2, ?10) AND c = '?'"),
            "WHERE a = $1 AND b IN ($2, $10) AND c = '?'"
        );
    }
}
