/// Serializa un tipo como texto usando su `Display` y lo lee con `FromStr`.
macro_rules! serde_as_str {
    ($($ty:ty),+ $(,)?) => {$(
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    )+};
}

pub(crate) use serde_as_str;
