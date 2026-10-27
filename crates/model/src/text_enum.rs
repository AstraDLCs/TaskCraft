/// Enum con representación de texto estable (almacenamiento, CLI, MCP).
/// Cada variante tiene su nombre canónico y alias opcionales en español.
/// `$error` es la clave i18n del error de lectura y `$prefix` el prefijo de la
/// etiqueta visible de cada variante (`status` → `status-in_progress`).
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident ($error:literal, $prefix:literal) {
            $($(#[$vmeta:meta])* $variant:ident = $text:literal $(| $alias:literal)*),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant),+];
            /// Nombres canónicos, en orden.
            pub const NAMES: &[&str] = &[$($text),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            /// Clave i18n del nombre visible de la variante.
            pub fn label_key(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!($prefix, "-", $text)),+
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::ParseEnumError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let lower = s.trim().to_lowercase().replace([' ', '-'], "_");
                match lower.as_str() {
                    $($text $(| $alias)* => Ok(Self::$variant),)+
                    _ => Err($crate::ParseEnumError {
                        key: $error,
                        value: s.to_owned(),
                        expected: Self::NAMES,
                    }),
                }
            }
        }

        $crate::serde_str::serde_as_str!($name);
    };
}

pub(crate) use text_enum;
