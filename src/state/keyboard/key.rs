use serde::de::{self, Visitor};
use serde::Deserialize;
use std::fmt;

use crate::state::actions::get_action;

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub enum RawKey {
    Key(String),
    Enigo(enigo::Key),
    Skip,
    Action(String),
    // FIXME we want this to to be an action, but there are special cases depending on it, we need to fix those first
    Shift,
}

fn raw_key_from_config_str(s: &str) -> RawKey {
    // Leading `\` forces a literal [`RawKey::Key`] (no keyword / action / enigo parsing).
    // Use `\\...` when the literal should begin with `\`.
    if let Some(rest) = s.strip_prefix('\\') {
        return RawKey::Key(rest.to_owned());
    }
    if s.eq_ignore_ascii_case("skip") {
        return RawKey::Skip;
    }
    if s.eq_ignore_ascii_case("shift") {
        return RawKey::Shift;
    }
    if get_action(s).is_some() {
        return RawKey::Action(s.to_owned());
    }
    if let Ok(k) = serde_plain::from_str::<enigo::Key>(s) {
        return RawKey::Enigo(k);
    }
    RawKey::Key(s.to_owned())
}

impl<'de> Deserialize<'de> for RawKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawKeyVisitor;

        impl<'de> Visitor<'de> for RawKeyVisitor {
            type Value = RawKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string key specifier")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(raw_key_from_config_str(value))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(raw_key_from_config_str(&value))
            }
        }

        deserializer.deserialize_any(RawKeyVisitor)
    }
}

impl PartialEq<str> for RawKey {
    fn eq(&self, other: &str) -> bool {
        match self {
            RawKey::Key(k) => k == other,
            RawKey::Action(a) if other.starts_with("action:") => {
                a == other.strip_prefix("action:").unwrap()
            }
            _ => false,
        }
    }
}

impl PartialEq<RawKey> for str {
    fn eq(&self, other: &RawKey) -> bool {
        other == self
    }
}

impl PartialEq<&str> for RawKey {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

pub trait ToShifted {
    fn to_shifted(&self) -> Self;
}

impl ToShifted for RawKey {
    fn to_shifted(&self) -> Self {
        match self {
            RawKey::Key(k) => RawKey::Key(k.to_uppercase()),
            _ => self.clone(),
        }
    }
}

impl ToShifted for String {
    fn to_shifted(&self) -> Self {
        self.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Key<T> {
    pub normal: T,
    pub shift: Option<T>,
}

impl<T: Clone + ToShifted> Key<T> {
    pub fn get(&self, shifted: bool) -> T {
        if shifted {
            if let Some(k) = &self.shift {
                k.clone()
            } else {
                self.normal.to_shifted()
            }
        } else {
            self.normal.clone()
        }
    }
}

// Helper struct for deserializing Key from table format
#[derive(Deserialize)]
struct KeyTable<T> {
    normal: T,
    shift: T,
}

impl<'de, T> Deserialize<'de> for Key<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;
        use std::marker::PhantomData;

        struct KeyVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for KeyVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Key<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string or a table with 'normal' and 'shift' fields")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Deserialize the string as T
                let normal = T::deserialize(de::value::StrDeserializer::new(value))?;
                Ok(Key {
                    normal,
                    shift: None,
                })
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Deserialize as a table with normal and shift fields
                let table = KeyTable::<T>::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Key {
                    normal: table.normal,
                    shift: Some(table.shift),
                })
            }
        }

        deserializer.deserialize_any(KeyVisitor(PhantomData))
    }
}
