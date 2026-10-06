use anyhow::Result;
use egui::Context;

use crate::surface_shape::SurfaceCorners;

#[derive(Default)]
pub(crate) struct WindowRegion {
    #[cfg(target_os = "windows")]
    last: Option<RegionKey>,
    applied: bool,
}

#[cfg(target_os = "windows")]
#[derive(PartialEq)]
struct RegionKey {
    handle: isize,
    width: i32,
    height: i32,
    pixels_per_point: u32,
    corners: Option<SurfaceCorners>,
}

impl WindowRegion {
    #[cfg(target_os = "windows")]
    pub(crate) fn sync(
        &mut self,
        frame: &eframe::Frame,
        ctx: &Context,
        corners: Option<&SurfaceCorners>,
    ) -> Result<bool> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let handle = frame.window_handle()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Ok(false);
        };
        self.sync_handle(
            handle.hwnd.get() as windows_sys::Win32::Foundation::HWND,
            ctx.pixels_per_point(),
            corners,
        )
    }

    #[cfg(target_os = "windows")]
    fn sync_handle(
        &mut self,
        hwnd: windows_sys::Win32::Foundation::HWND,
        scale: f32,
        corners: Option<&SurfaceCorners>,
    ) -> Result<bool> {
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::{
            CreatePolygonRgn, DeleteObject, SetWindowRgn, WINDING,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

        let mut bounds = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut bounds) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let key = RegionKey {
            handle: hwnd as isize,
            width: bounds.right - bounds.left,
            height: bounds.bottom - bounds.top,
            pixels_per_point: scale.to_bits(),
            corners: corners.cloned(),
        };
        if self.last.as_ref() == Some(&key) {
            return Ok(self.applied);
        }
        self.last = Some(key);
        self.applied = false;
        if corners.is_none() && unsafe { SetWindowRgn(hwnd, std::ptr::null_mut(), 1) } != 0 {
            return Ok(false);
        }
        let Some(corners) = corners else {
            return Err(std::io::Error::last_os_error().into());
        };
        let rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                (bounds.right - bounds.left) as f32 / scale,
                (bounds.bottom - bounds.top) as f32 / scale,
            ),
        );
        let points: Vec<POINT> = corners
            .outline(rect)
            .into_iter()
            .map(|point| POINT {
                x: (point.x * scale).round() as i32,
                y: (point.y * scale).round() as i32,
            })
            .collect();
        let region = unsafe { CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING) };
        if region.is_null() {
            let error = std::io::Error::last_os_error();
            unsafe { SetWindowRgn(hwnd, std::ptr::null_mut(), 1) };
            return Err(error.into());
        }
        if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                DeleteObject(region);
                SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
            }
            return Err(error.into());
        }
        self.applied = true;
        Ok(true)
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn sync(
        &mut self,
        _frame: &eframe::Frame,
        _ctx: &Context,
        _corners: Option<&SurfaceCorners>,
    ) -> Result<bool> {
        self.applied = false;
        Ok(false)
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        WS_POPUP,
    };

    #[test]
    fn native_region_tracks_size_scale_and_restores_rectangle() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                100,
                60,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        struct TestWindow(windows_sys::Win32::Foundation::HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0) };
            }
        }
        let window = TestWindow(hwnd);
        let corners: SurfaceCorners =
            toml::from_str("corner_style='beveled'\n[beveled_corners]\ncorner_size=12").unwrap();
        let mut adapter = WindowRegion::default();
        for scale in [1.0, 1.25, 2.0] {
            assert!(adapter
                .sync_handle(window.0, scale, Some(&corners))
                .unwrap());
            let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
            assert_ne!(unsafe { GetWindowRgn(window.0, region) }, 0);
            assert_eq!(unsafe { PtInRegion(region, 1, 1) }, 0);
            assert_ne!(unsafe { PtInRegion(region, 50, 30) }, 0);
            assert_eq!(
                unsafe { PtInRegion(region, 100 - (6.0 * scale) as i32, 1) },
                0
            );
            unsafe { DeleteObject(region) };
        }
        assert_ne!(
            unsafe {
                SetWindowPos(
                    window.0,
                    std::ptr::null_mut(),
                    0,
                    0,
                    160,
                    90,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
                )
            },
            0
        );
        assert!(adapter.sync_handle(window.0, 1.0, Some(&corners)).unwrap());
        let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
        assert_ne!(unsafe { GetWindowRgn(window.0, region) }, 0);
        assert_ne!(unsafe { PtInRegion(region, 150, 80) }, 0);
        assert!(!adapter.sync_handle(window.0, 1.0, None).unwrap());
        assert_eq!(unsafe { GetWindowRgn(window.0, region) }, 0);
        unsafe { DeleteObject(region) };
    }
}
