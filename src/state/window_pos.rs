use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

const POINTER_GAP: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum PointerH {
    #[default]
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum PointerV {
    /// Top of the window at the cursor (hangs down).
    #[default]
    Bottom,
    /// Bottom of the window just above the cursor (hangs up).
    Top,
}

/// One of the four corners around the captured pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct PointerPlacement {
    horizontal: PointerH,
    vertical: PointerV,
}

/// Counterclockwise around the cursor: BR → BL → TL → TR.
const SLOT_RING: [PointerPlacement; 4] = [
    PointerPlacement {
        horizontal: PointerH::Right,
        vertical: PointerV::Bottom,
    },
    PointerPlacement {
        horizontal: PointerH::Left,
        vertical: PointerV::Bottom,
    },
    PointerPlacement {
        horizontal: PointerH::Left,
        vertical: PointerV::Top,
    },
    PointerPlacement {
        horizontal: PointerH::Right,
        vertical: PointerV::Top,
    },
];

/// Captured cursor and work-area, in physical pixels (virtual screen).
#[derive(Clone, Copy, Debug)]
pub struct PointerSnapshot {
    cursor_px: (i32, i32),
    work_px: (i32, i32, i32, i32),
    placement: Option<PointerPlacement>,
}

impl PointerSnapshot {
    fn geom(&self, window_size: (f32, f32), pixels_per_point: f32) -> PointerGeom {
        let ppp = pixels_per_point.max(0.1);
        let pt = |v: i32| v as f32 / ppp;
        let (l, t, r, b) = self.work_px;
        PointerGeom {
            cursor: (pt(self.cursor_px.0), pt(self.cursor_px.1)),
            window: window_size,
            origin: (pt(l), pt(t)),
            size: (pt(r - l), pt(b - t)),
        }
    }

    fn ensure_placement(&mut self, geom: PointerGeom) -> PointerPlacement {
        *self
            .placement
            .get_or_insert_with(|| choose_default_placement(geom))
    }

    /// Place the window next to the captured pointer, in egui points.
    pub fn coords_points(&mut self, window_size: (f32, f32), pixels_per_point: f32) -> (f32, f32) {
        let geom = self.geom(window_size, pixels_per_point);
        let placement = self.ensure_placement(geom);
        place_near_pointer(geom, placement)
    }

    pub fn flip_horizontal(&mut self, window_size: (f32, f32), pixels_per_point: f32) {
        let geom = self.geom(window_size, pixels_per_point);
        let current = self.ensure_placement(geom);
        self.placement = Some(flip_horizontal(current));
    }

    pub fn flip_vertical(&mut self, window_size: (f32, f32), pixels_per_point: f32) {
        let geom = self.geom(window_size, pixels_per_point);
        let current = self.ensure_placement(geom);
        self.placement = Some(flip_vertical(current));
    }

    /// Advance to the next of the four slots around the pointer.
    pub fn rotate(&mut self, window_size: (f32, f32), pixels_per_point: f32) {
        let geom = self.geom(window_size, pixels_per_point);
        let current = self.ensure_placement(geom);
        let i = SLOT_RING
            .iter()
            .position(|&slot| slot == current)
            .unwrap_or(0);
        self.placement = Some(SLOT_RING[(i + 1) % SLOT_RING.len()]);
    }
}

/// Snapshot the pointer and the work area of the monitor that contains it.
pub fn capture_pointer_snapshot() -> Option<PointerSnapshot> {
    #[cfg(windows)]
    {
        capture_pointer_snapshot_win()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn capture_pointer_snapshot_win() -> Option<PointerSnapshot> {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rcWork: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dwFlags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let work = info.rcWork;
        Some(PointerSnapshot {
            cursor_px: (pt.x, pt.y),
            work_px: (work.left, work.top, work.right, work.bottom),
            placement: None,
        })
    }
}

/// Clamp window top-left so the full window stays inside a work rect at the origin.
pub fn clamp_coords(
    coords: (f32, f32),
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (f32, f32) {
    clamp_to_rect(coords, window_size, (0.0, 0.0), monitor_size)
}

fn clamp_to_rect(
    coords: (f32, f32),
    window_size: (f32, f32),
    origin: (f32, f32),
    size: (f32, f32),
) -> (f32, f32) {
    let (win_w, win_h) = window_size;
    let max_x = origin.0 + (size.0 - win_w).max(0.0);
    let max_y = origin.1 + (size.1 - win_h).max(0.0);
    (
        coords.0.clamp(origin.0, max_x),
        coords.1.clamp(origin.1, max_y),
    )
}

#[derive(Clone, Copy)]
struct PointerGeom {
    cursor: (f32, f32),
    window: (f32, f32),
    origin: (f32, f32),
    size: (f32, f32),
}

fn fits_right(geom: PointerGeom) -> bool {
    let (cx, _) = geom.cursor;
    (geom.origin.0 + geom.size.0) - cx >= geom.window.0 + POINTER_GAP
}

fn fits_below(geom: PointerGeom) -> bool {
    let (_, cy) = geom.cursor;
    (geom.origin.1 + geom.size.1) - cy >= geom.window.1
}

fn choose_default_placement(geom: PointerGeom) -> PointerPlacement {
    PointerPlacement {
        horizontal: if fits_right(geom) {
            PointerH::Right
        } else {
            PointerH::Left
        },
        vertical: if fits_below(geom) {
            PointerV::Bottom
        } else {
            PointerV::Top
        },
    }
}

fn flip_horizontal(current: PointerPlacement) -> PointerPlacement {
    PointerPlacement {
        horizontal: match current.horizontal {
            PointerH::Right => PointerH::Left,
            PointerH::Left => PointerH::Right,
        },
        vertical: current.vertical,
    }
}

fn flip_vertical(current: PointerPlacement) -> PointerPlacement {
    PointerPlacement {
        horizontal: current.horizontal,
        vertical: match current.vertical {
            PointerV::Bottom => PointerV::Top,
            PointerV::Top => PointerV::Bottom,
        },
    }
}

/// Sit beside the pointer. Horizontal and vertical offsets are independent.
/// Result is clamped to the work area so the full window stays on screen.
fn place_near_pointer(geom: PointerGeom, placement: PointerPlacement) -> (f32, f32) {
    let (cx, cy) = geom.cursor;
    let (ww, wh) = geom.window;
    let gap = POINTER_GAP;
    let x = match placement.horizontal {
        PointerH::Right => cx + gap,
        PointerH::Left => cx - gap - ww,
    };
    let y = match placement.vertical {
        PointerV::Bottom => cy,
        PointerV::Top => cy - gap - wh,
    };
    clamp_to_rect((x, y), geom.window, geom.origin, geom.size)
}

/// Resolve a saved position to on-screen coordinates for the current monitor and window size.
///
/// Corner presets are recomputed each time so they stay anchored after display scaling changes.
/// Absolute coordinates are clamped when they no longer fit (e.g. after a DPI change).
/// `MousePointer` is resolved by [`AppState`] from a pointer snapshot, not here.
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
        WindowPos::MousePointer => ((max_x, max_y), pos),
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
    MousePointer,
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
            WindowPos::MousePointer => serializer.serialize_str("mouse pointer"),
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
                formatter.write_str(
                    "a string ('top left', 'top right', 'bottom right', 'bottom left', 'mouse pointer') or an [x, y] array",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<WindowPos, E>
            where
                E: de::Error,
            {
                match value.to_lowercase().replace(' ', "").as_str() {
                    "topleft" => Ok(WindowPos::TopLeft),
                    "topright" => Ok(WindowPos::TopRight),
                    "bottomright" => Ok(WindowPos::BottomRight),
                    "bottomleft" => Ok(WindowPos::BottomLeft),
                    "mousepointer" => Ok(WindowPos::MousePointer),
                    _ => Err(de::Error::unknown_variant(
                        value,
                        &[
                            "top left",
                            "top right",
                            "bottom right",
                            "bottom left",
                            "mouse pointer",
                        ],
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

        let (_, at_100) = resolve_position(WindowPos::BottomRight, window, monitor_100);
        let (_, at_150) = resolve_position(WindowPos::BottomRight, window, monitor_150);

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

    #[test]
    fn parses_mouse_pointer() {
        #[derive(Deserialize)]
        struct Cfg {
            window_pos: WindowPos,
        }
        let cfg: Cfg = toml::from_str("window_pos = \"MousePointer\"").unwrap();
        assert_eq!(cfg.window_pos, WindowPos::MousePointer);
        let cfg: Cfg = toml::from_str("window_pos = \"mouse pointer\"").unwrap();
        assert_eq!(cfg.window_pos, WindowPos::MousePointer);
    }

    fn geom(cursor: (f32, f32), window: (f32, f32), work: (f32, f32)) -> PointerGeom {
        PointerGeom {
            cursor,
            window,
            origin: (0.0, 0.0),
            size: work,
        }
    }

    #[test]
    fn pointer_prefers_right_of_cursor() {
        let window = (400.0, 200.0);
        let work = (1920.0, 1080.0);
        let cursor = (200.0, 100.0);
        let g = geom(cursor, window, work);
        let place = choose_default_placement(g);
        assert_eq!(place.horizontal, PointerH::Right);
        assert_eq!(place.vertical, PointerV::Bottom);
        let pos = place_near_pointer(g, place);
        assert!((pos.0 - (cursor.0 + POINTER_GAP)).abs() < 0.1, "{pos:?}");
        assert!((pos.1 - cursor.1).abs() < 0.1, "{pos:?}");
    }

    #[test]
    fn pointer_falls_back_left_when_no_right_room() {
        let window = (400.0, 200.0);
        let work = (1920.0, 1080.0);
        let cursor = (1900.0, 100.0);
        let g = geom(cursor, window, work);
        let place = choose_default_placement(g);
        assert_eq!(place.horizontal, PointerH::Left);
        assert_eq!(place.vertical, PointerV::Bottom);
        let pos = place_near_pointer(g, place);
        assert!(pos.0 + window.0 <= cursor.0, "{pos:?}");
    }

    #[test]
    fn pointer_falls_back_top_when_no_room_below() {
        let window = (400.0, 200.0);
        let work = (1920.0, 1080.0);
        let cursor = (200.0, 950.0);
        let g = geom(cursor, window, work);
        let place = choose_default_placement(g);
        assert_eq!(place.horizontal, PointerH::Right);
        assert_eq!(place.vertical, PointerV::Top);
    }

    #[test]
    fn flip_horizontal_keeps_vertical() {
        let g = geom((500.0, 400.0), (400.0, 200.0), (1920.0, 1080.0));
        let start = choose_default_placement(g);
        assert_eq!(start.vertical, PointerV::Bottom);
        let top_right = flip_vertical(start);
        assert_eq!(top_right.vertical, PointerV::Top);
        assert_eq!(top_right.horizontal, PointerH::Right);

        let top_left = flip_horizontal(top_right);
        assert_eq!(top_left.horizontal, PointerH::Left);
        assert_eq!(top_left.vertical, PointerV::Top);

        let top_right_again = flip_horizontal(top_left);
        assert_eq!(top_right_again, top_right);
    }

    #[test]
    fn flip_vertical_toggles_top_and_bottom() {
        let window = (400.0, 200.0);
        let cursor = (500.0, 400.0);
        let g = geom(cursor, window, (1920.0, 1080.0));
        let start = choose_default_placement(g);
        assert_eq!(start.vertical, PointerV::Bottom);
        let top = flip_vertical(start);
        assert_eq!(top.vertical, PointerV::Top);
        assert_eq!(top.horizontal, PointerH::Right);
        let top_pos = place_near_pointer(g, top);
        assert!(
            (top_pos.1 - (cursor.1 - POINTER_GAP - window.1)).abs() < 0.1,
            "{top_pos:?}"
        );
        let bottom_again = flip_vertical(top);
        assert_eq!(bottom_again, start);
    }

    fn assert_inside_work(pos: (f32, f32), window: (f32, f32), work: (f32, f32)) {
        assert!(pos.0 >= -0.1 && pos.1 >= -0.1, "{pos:?}");
        assert!(pos.0 + window.0 <= work.0 + 0.1, "{pos:?}");
        assert!(pos.1 + window.1 <= work.1 + 0.1, "{pos:?}");
    }

    #[test]
    fn pointer_stays_inside_work_area() {
        let window = (520.0, 250.0);
        let work = (800.0, 600.0);
        let g = geom((10.0, 10.0), window, work);
        let pos = place_near_pointer(g, choose_default_placement(g));
        assert_inside_work(pos, window, work);
    }

    #[test]
    fn slot_that_would_go_off_screen_is_clamped_inside() {
        let window = (400.0, 200.0);
        let work = (1920.0, 1080.0);
        let cursor = (100.0, 50.0);
        let g = geom(cursor, window, work);
        let top_left = PointerPlacement {
            horizontal: PointerH::Left,
            vertical: PointerV::Top,
        };
        let pos = place_near_pointer(g, top_left);
        assert_inside_work(pos, window, work);
        assert!(pos.0 >= 0.0 && pos.1 >= 0.0);
    }

    fn snapshot(cursor: (i32, i32), work: (i32, i32, i32, i32)) -> PointerSnapshot {
        PointerSnapshot {
            cursor_px: cursor,
            work_px: work,
            placement: None,
        }
    }

    #[test]
    fn rotate_cycles_four_slots_near_cursor() {
        let window = (400.0, 200.0);
        let ppp = 1.0;
        let mut snap = snapshot((500, 400), (0, 0, 1920, 1080));
        let start = snap.coords_points(window, ppp);
        assert_eq!(snap.placement.unwrap().horizontal, PointerH::Right);
        assert_eq!(snap.placement.unwrap().vertical, PointerV::Bottom);

        snap.rotate(window, ppp);
        let first = snap.placement.unwrap();
        assert_eq!(first.horizontal, PointerH::Left);
        assert_eq!(first.vertical, PointerV::Bottom);
        let bl = snap.coords_points(window, ppp);
        assert!(
            (bl.0 - (500.0 - POINTER_GAP - window.0)).abs() < 0.1,
            "{bl:?}"
        );
        assert!((bl.1 - 400.0).abs() < 0.1, "{bl:?}");

        snap.rotate(window, ppp);
        let second = snap.placement.unwrap();
        assert_eq!(second.horizontal, PointerH::Left);
        assert_eq!(second.vertical, PointerV::Top);
        let tl = snap.coords_points(window, ppp);
        assert!(
            (tl.1 - (400.0 - POINTER_GAP - window.1)).abs() < 0.1,
            "{tl:?}"
        );

        snap.rotate(window, ppp);
        let third = snap.placement.unwrap();
        assert_eq!(third.horizontal, PointerH::Right);
        assert_eq!(third.vertical, PointerV::Top);

        snap.rotate(window, ppp);
        let fourth = snap.placement.unwrap();
        assert_eq!(fourth.horizontal, PointerH::Right);
        assert_eq!(fourth.vertical, PointerV::Bottom);
        let back = snap.coords_points(window, ppp);
        assert!(
            (back.0 - start.0).abs() < 0.1 && (back.1 - start.1).abs() < 0.1,
            "{back:?} vs {start:?}"
        );
    }

    #[test]
    fn flip_then_rotate_stays_on_the_four_slots() {
        let window = (400.0, 200.0);
        let ppp = 1.0;
        let mut snap = snapshot((500, 400), (0, 0, 1920, 1080));

        snap.flip_horizontal(window, ppp);
        assert_eq!(snap.placement.unwrap().horizontal, PointerH::Left);
        assert_eq!(snap.placement.unwrap().vertical, PointerV::Bottom);

        snap.rotate(window, ppp);
        let after = snap.placement.unwrap();
        assert_eq!(after.horizontal, PointerH::Left);
        assert_eq!(after.vertical, PointerV::Top);
    }
}
