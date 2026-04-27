use std::hint::unreachable_unchecked;

use serde::Deserialize;

#[derive(PartialEq, Eq, Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub enum RawKey {
    Key(String),
    Enigo(enigo::Key),
    Skip,
    Shift,
    Ctrl,
    Alt,
    Paste,
    Done,
    Menu,
    TextInput,
    SwitchLayout(String),
}

impl TryFrom<String> for RawKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 1 {
            return Ok(RawKey::Key(value));
        }

        let upper = value.to_uppercase();

        match upper.as_str() {
            "SKIP" => return Ok(RawKey::Skip),
            "SHIFT" => return Ok(RawKey::Shift),
            "CTRL" => return Ok(RawKey::Ctrl),
            "ALT" => return Ok(RawKey::Alt),
            "PASTE" => return Ok(RawKey::Paste),
            "DONE" => return Ok(RawKey::Done),
            "MENU" => return Ok(RawKey::Menu),
            "TEXT-INPUT" => return Ok(RawKey::TextInput),
            _ => {}
        }

        // Check for layout switch prefix
        if let Some(layout_name) = value.strip_prefix("layout:") {
            return Ok(RawKey::SwitchLayout(layout_name.to_string()));
        }

        if let Ok(key) = serde_plain::from_str::<enigo::Key>(&value) {
            return Ok(RawKey::Enigo(key));
        }

        Err(format!("unknown key '{}'", value))
    }
}

impl PartialEq<str> for RawKey {
    fn eq(&self, other: &str) -> bool {
        match self {
            RawKey::Key(k) => k == other,
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

impl std::fmt::Display for RawKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RawKey::Key(k) => write!(f, "{}", k),
            RawKey::Enigo(k) => write!(f, "{:?}", k),
            RawKey::Skip => unsafe { unreachable_unchecked() },
            RawKey::Shift => write!(f, "Shift"),
            RawKey::Ctrl => write!(f, "Ctrl"),
            RawKey::Alt => write!(f, "Alt"),
            RawKey::Paste => write!(f, "Paste"),
            RawKey::Done => write!(f, "Done"),
            RawKey::Menu => write!(f, "Menu"),
            RawKey::TextInput => write!(f, "TextInput"),
            RawKey::SwitchLayout(name) => write!(f, "layout:{}", name),
        }
    }
}

pub trait ToShifted {
    fn to_shifted(&self) -> Self;
}

impl ToShifted for RawKey {
    fn to_shifted(&self) -> Self {
        match self {
            RawKey::Key(k) => RawKey::Key(k.to_uppercase()),
            RawKey::SwitchLayout(name) => RawKey::SwitchLayout(name.clone()),
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

impl<T: ToString + Clone + ToShifted> Key<T> {
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
