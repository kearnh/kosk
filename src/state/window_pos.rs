use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum WindowPos {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Absolute(f32, f32),
}

impl Serialize for WindowPos {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            WindowPos::TopLeft => serializer.serialize_str("top left"),
            WindowPos::TopRight => serializer.serialize_str("top right"),
            WindowPos::BottomRight => serializer.serialize_str("bottom right"),
            WindowPos::BottomLeft => serializer.serialize_str("bottom left"),
            WindowPos::Absolute(x, y) => (x, y).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for WindowPos {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WindowPosVisitor;

        impl<'de> de::Visitor<'de> for WindowPosVisitor {
            type Value = WindowPos;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string ('top left', 'top right', 'bottom right', 'bottom left') or an [x, y] array")
            }

            fn visit_str<E>(self, value: &str) -> Result<WindowPos, E>
            where
                E: de::Error,
            {
                match value.to_lowercase().as_str() {
                    "top left" => Ok(WindowPos::TopLeft),
                    "top right" => Ok(WindowPos::TopRight),
                    "bottom right" => Ok(WindowPos::BottomRight),
                    "bottom left" => Ok(WindowPos::BottomLeft),
                    _ => Err(de::Error::unknown_variant(
                        value,
                        &["top left", "top right", "bottom right", "bottom left"],
                    )),
                }
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<WindowPos, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let x: f32 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let y: f32 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(WindowPos::Absolute(x, y))
            }

            fn visit_map<M>(self, mut map: M) -> Result<WindowPos, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "Absolute" => {
                            let coords: (f32, f32) = map.next_value()?;
                            return Ok(WindowPos::Absolute(coords.0, coords.1));
                        }
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Err(de::Error::custom("Expected 'Absolute' key in map"))
            }
        }

        deserializer.deserialize_any(WindowPosVisitor)
    }
}
