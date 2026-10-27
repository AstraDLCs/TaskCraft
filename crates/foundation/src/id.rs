/// Declara un ID tipado respaldado por un ULID: ordenable por tiempo,
/// único sin coordinación y legible como texto de 26 caracteres.
#[macro_export]
macro_rules! ulid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($crate::Ulid);

        impl $name {
            pub fn generate() -> Self {
                Self($crate::next_ulid())
            }

            pub fn from_ulid(ulid: $crate::Ulid) -> Self {
                Self(ulid)
            }

            pub fn ulid(&self) -> $crate::Ulid {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::DecodeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse().map(Self)
            }
        }

        /// Se serializa como el texto ULID de 26 caracteres.
        impl $crate::serde::Serialize for $name {
            fn serialize<S: $crate::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(&self.0)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            fn deserialize<D: $crate::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <::std::borrow::Cow<'de, str> as $crate::serde::Deserialize>::deserialize(d)?;
                text.parse().map_err(<D::Error as $crate::serde::de::Error>::custom)
            }
        }
    };
}

ulid_id! {
    /// Agrupa todos los logs y eventos producidos por un mismo comando.
    CorrelationId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_text() {
        let id = CorrelationId::generate();
        let parsed: CorrelationId = id.to_string().parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn rejects_garbage() {
        assert!("no-es-un-ulid".parse::<CorrelationId>().is_err());
    }
}
