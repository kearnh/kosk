use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Clamp window top-left coordinates so the full window stays within the monitor.
pub fn clamp_coords(
    coords: (f32, f32),
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (f32, f32) {
    let (win_w, win_h) = window_size;
    let (mon_w, mon_h) = monitor_size;
    let max_x = (mon_w - win_w).max(0.0);
    let max_y = (mon_h - win_h).max(0.0);
    (coords.0.clamp(0.0, max_x), coords.1.clamp(0.0, max_y))
}

/// Resolve a saved position to on-screen coordinates for the current monitor and window size.
///
/// Corner presets are recomputed each time so they stay anchored after display scaling changes.
/// Absolute coordinates are clamped when they no longer fit (e.g. after a DPI change).
pub fn resolve_position(
    pos: WindowPos,
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (WindowPos, (f32, f32)) {
    let (win_w, win_h) = window_size;
    let (mon_w, mon_h) = monitor_size;
    let max_x = (mon_w - win_w).max(0.0);
    let max_y = (mon_h - win_h).max(0.0);

    let (coords, resolved) = match pos {
        WindowPos::Absolute(x, y) => {
            let clamped = clamp_coords((x, y), window_size, monitor_size);
            let resolved = if clamped != (x, y) {
                WindowPos::Absolute(clamped.0, clamped.1)
            } else {
                pos
            };
            (clamped, resolved)
        }
        WindowPos::TopLeft => ((0.0, 0.0), pos),
        WindowPos::TopRight => ((max_x, 0.0), pos),
        WindowPos::BottomRight => ((max_x, max_y), pos),
        WindowPos::BottomLeft => ((0.0, max_y), pos),
    };

    let coords = clamp_coords(coords, window_size, monitor_size);
    (resolved, coords)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_bottom_right_tracks_monitor_size() {
        let window = (520.0, 250.0);
        let monitor_100 = (1920.0, 1080.0);
        let monitor_150 = (1280.0, 720.0);

        let (_, at_100) =
            resolve_position(WindowPos::BottomRight, window, monitor_100);
        let (_, at_150) =
            resolve_position(WindowPos::BottomRight, window, monitor_150);

        assert_eq!(at_100, (1400.0, 830.0));
        assert_eq!(at_150, (760.0, 470.0));
    }

    #[test]
    fn absolute_position_clamps_after_scale_change() {
        let window = (520.0, 250.0);
        let monitor_150 = (1280.0, 720.0);
        let saved_at_100 = WindowPos::Absolute(1994.0, 1087.0);

        let (resolved, coords) = resolve_position(saved_at_100, window, monitor_150);

        assert_eq!(coords, (760.0, 470.0));
        assert_eq!(resolved, WindowPos::Absolute(760.0, 470.0));
    }
}
