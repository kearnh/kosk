use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::{bail, Context, Result};
use egui::{Color32, ColorImage, Mesh, Rect, TextureHandle, TextureOptions, Ui, Vec2};

use crate::config::Config;
use crate::state::StateId;
use crate::theme::{BackgroundImagePosition, BackgroundImageScaling, BackgroundImageTheme};

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 4096;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
struct ImageKey {
    path: PathBuf,
    revision: u64,
}

struct PendingImage {
    key: ImageKey,
    result: Receiver<Result<ColorImage>>,
}

#[derive(Default)]
pub(crate) struct Background {
    active: Option<ImageKey>,
    texture: Option<TextureHandle>,
    pending: Option<PendingImage>,
    attempted: bool,
}

impl Background {
    pub(crate) fn draw(&mut self, ui: &Ui, cfg: &Config, state: StateId) {
        let image = cfg
            .theme()
            .background_image
            .as_ref()
            .filter(|image| state != StateId::MoveWindow && image.opacity > 0.0);
        let desired = image.map(|image| ImageKey {
            path: image.path.clone(),
            revision: cfg.theme_revision(),
        });
        if self.active != desired {
            self.active = desired;
            self.texture = None;
            self.attempted = false;
        }

        self.finish_loading(ui);
        if self.pending.is_none()
            && !self.attempted
            && let Some(key) = self.active.clone()
        {
            self.attempted = true;
            self.start_loading(ui, key);
        }

        let (Some(image), Some(texture)) = (image, &self.texture) else {
            return;
        };
        let viewport = ui.ctx().content_rect();
        if !viewport.is_positive() {
            return;
        }
        let window_alpha = cfg.window_visuals(state).panel_fill.a() as f32 / u8::MAX as f32;
        let alpha = (image.opacity * window_alpha * u8::MAX as f32).round() as u8;
        let mesh = match image_mesh(viewport, texture, image, Color32::from_white_alpha(alpha)) {
            Ok(mesh) => mesh,
            Err(error) => {
                self.texture = None;
                report_failure(&image.path, &error.to_string());
                return;
            }
        };
        ui.painter().with_clip_rect(viewport).add(mesh);
    }

    fn start_loading(&mut self, ui: &Ui, key: ImageKey) {
        let (tx, result) = mpsc::channel();
        let path = key.path.clone();
        let ctx = ui.ctx().clone();
        let worker = std::thread::Builder::new()
            .name("theme-background".to_owned())
            .spawn(move || {
                let _ = tx.send(load_image(&path));
                ctx.request_repaint();
            });
        match worker {
            Ok(_) => self.pending = Some(PendingImage { key, result }),
            Err(error) => report_failure(&key.path, &error.to_string()),
        }
    }

    fn finish_loading(&mut self, ui: &Ui) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("image worker stopped")),
        };
        let pending = self.pending.take().expect("pending image");
        if self.active.as_ref() != Some(&pending.key) {
            return;
        }
        match result {
            Ok(image) => {
                self.texture = Some(ui.ctx().load_texture(
                    pending.key.path.to_string_lossy(),
                    image,
                    TextureOptions::LINEAR,
                ));
            }
            Err(error) => report_failure(&pending.key.path, &format!("{error:#}")),
        }
    }
}

fn report_failure(path: &Path, error: &str) {
    eprintln!("theme background {}: {error}", path.display());
    crate::user_notify::notify(crate::user_notify::Notice::theme_background_failed(path));
}

fn image_mesh(
    viewport: Rect,
    texture: &TextureHandle,
    image: &BackgroundImageTheme,
    tint: Color32,
) -> Result<Mesh> {
    let texture_size = texture.size_vec2();
    let source = match image.source_region {
        Some([x, y, width, height]) => Rect::from_min_size(
            egui::pos2(x as f32, y as f32),
            egui::vec2(width as f32, height as f32),
        ),
        None => Rect::from_min_size(egui::Pos2::ZERO, texture_size),
    };
    if !source.is_positive()
        || !Rect::from_min_size(egui::Pos2::ZERO, texture_size).contains_rect(source)
    {
        bail!("background_image.source_region exceeds image dimensions");
    }
    let mut mesh = Mesh::with_texture(texture.id());
    let border = image.frame_border as f32;
    if border == 0.0 {
        let uv = Rect::from_min_max(
            egui::Pos2::ZERO + source.min.to_vec2() / texture_size,
            egui::Pos2::ZERO + source.max.to_vec2() / texture_size,
        );
        mesh.add_rect_with_uv(image_rect(viewport, source.size(), image), uv, tint);
        return Ok(mesh);
    }
    if border * 2.0 >= source.width() || border * 2.0 >= source.height() {
        bail!("background_image.frame_border must leave space inside the source image");
    }

    let horizontal_border = border.min(viewport.width() / 2.0);
    let vertical_border = border.min(viewport.height() / 2.0);
    let x = [
        viewport.left(),
        viewport.left() + horizontal_border,
        viewport.right() - horizontal_border,
        viewport.right(),
    ];
    let y = [
        viewport.top(),
        viewport.top() + vertical_border,
        viewport.bottom() - vertical_border,
        viewport.bottom(),
    ];
    let u = [
        source.left(),
        source.left() + border,
        source.right() - border,
        source.right(),
    ]
    .map(|x| x / texture_size.x);
    let v = [
        source.top(),
        source.top() + border,
        source.bottom() - border,
        source.bottom(),
    ]
    .map(|y| y / texture_size.y);
    for row in 0..3 {
        for column in 0..3 {
            if row == 1 && column == 1 {
                continue;
            }
            let rect = Rect::from_min_max(
                egui::pos2(x[column], y[row]),
                egui::pos2(x[column + 1], y[row + 1]),
            );
            if !rect.is_positive() {
                continue;
            }
            let uv = Rect::from_min_max(
                egui::pos2(u[column], v[row]),
                egui::pos2(u[column + 1], v[row + 1]),
            );
            mesh.add_rect_with_uv(rect, uv, tint);
        }
    }
    Ok(mesh)
}

fn image_rect(viewport: Rect, source: Vec2, image: &BackgroundImageTheme) -> Rect {
    let ratios = viewport.size() / source;
    let size = match image.scaling {
        BackgroundImageScaling::Cover => source * ratios.x.max(ratios.y),
        BackgroundImageScaling::Contain => source * ratios.x.min(ratios.y),
        BackgroundImageScaling::Stretch => viewport.size(),
        BackgroundImageScaling::Original => source,
    };
    let align = match image.position {
        BackgroundImagePosition::Center => egui::Align2::CENTER_CENTER,
        BackgroundImagePosition::TopLeft => egui::Align2::LEFT_TOP,
        BackgroundImagePosition::TopRight => egui::Align2::RIGHT_TOP,
        BackgroundImagePosition::BottomLeft => egui::Align2::LEFT_BOTTOM,
        BackgroundImagePosition::BottomRight => egui::Align2::RIGHT_BOTTOM,
    };
    align.align_size_within_rect(size, viewport)
}

fn load_image(path: &Path) -> Result<ColorImage> {
    let file = std::fs::File::open(path).context("open background image")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        bail!("background image exceeds 16 MiB");
    }
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        return load_svg(&bytes);
    }

    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        bail!("background image must be PNG, JPEG or SVG");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .context("decode background image")?
        .into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

fn load_svg(bytes: &[u8]) -> Result<ColorImage> {
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).context("parse background SVG")?;
    let size = tree.size().to_int_size();
    if size.width() > MAX_IMAGE_DIMENSION || size.height() > MAX_IMAGE_DIMENSION {
        bail!("background SVG dimensions exceed {MAX_IMAGE_DIMENSION}");
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .context("allocate background SVG")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(ColorImage::from_rgba_premultiplied(
        [size.width() as usize, size.height() as usize],
        pixmap.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_image_uploads_once_and_stale_results_are_discarded() {
        let ctx = egui::Context::default();
        let key = ImageKey {
            path: "gear.png".into(),
            revision: 1,
        };
        let (tx, result) = mpsc::channel();
        tx.send(Ok(ColorImage::filled([2, 2], Color32::WHITE)))
            .unwrap();
        let mut background = Background {
            active: Some(key.clone()),
            pending: Some(PendingImage {
                key: key.clone(),
                result,
            }),
            attempted: true,
            ..Default::default()
        };
        let mut output = ctx.run_ui(Default::default(), |ui| background.finish_loading(ui));
        let texture = background.texture.as_ref().unwrap().id();
        assert!(output
            .textures_delta
            .set
            .iter()
            .any(|(id, _)| *id == texture));
        output.textures_delta.clear();
        let mut output = ctx.run_ui(Default::default(), |ui| background.finish_loading(ui));
        assert!(output.textures_delta.set.is_empty());
        output.textures_delta.clear();

        background.active = Some(ImageKey {
            revision: 2,
            ..key.clone()
        });
        background.texture = None;
        let (tx, result) = mpsc::channel();
        tx.send(Ok(ColorImage::filled([2, 2], Color32::WHITE)))
            .unwrap();
        background.pending = Some(PendingImage { key, result });
        let mut output = ctx.run_ui(Default::default(), |ui| background.finish_loading(ui));
        assert!(background.texture.is_none());
        assert!(output.textures_delta.set.is_empty());
        assert!(output.textures_delta.free.contains(&texture));
        output.textures_delta.clear();
    }

    #[test]
    fn raster_limits_corrupt_files_and_jpeg_support() {
        let dir =
            std::env::temp_dir().join(format!("kosk-background-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.png");
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(MAX_IMAGE_DIMENSION + 1, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&path, bytes.into_inner()).unwrap();
        assert!(load_image(&path).is_err());
        std::fs::write(&path, b"invalid image").unwrap();
        assert!(load_image(&path).is_err());
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(load_image(&path).is_err());

        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        std::fs::write(&path, bytes.into_inner()).unwrap();
        assert_eq!(load_image(&path).unwrap().size, [2, 2]);
        std::fs::remove_file(&path).unwrap();
        assert!(load_image(&path).is_err());
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn image_frame_preserves_corners_and_leaves_the_center_clear() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "frame",
            ColorImage::filled([64, 64], Color32::WHITE),
            TextureOptions::LINEAR,
        );
        let image = BackgroundImageTheme {
            source_region: Some([0, 0, 17, 17]),
            frame_border: 8,
            ..Default::default()
        };
        let viewport = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(300.0, 100.0));
        let mesh = image_mesh(viewport, &texture, &image, Color32::WHITE).unwrap();
        assert_eq!(mesh.vertices.len(), 32);
        assert_eq!(mesh.calc_bounds(), viewport);
        let mut area = 0.0;
        for vertices in mesh.vertices.as_chunks::<4>().0 {
            let rect = Rect::from_min_max(vertices[0].pos, vertices[3].pos);
            assert!(!rect.contains(viewport.center()));
            assert!(viewport.contains_rect(rect));
            area += rect.area();
        }
        assert_eq!(area, viewport.area() - viewport.shrink(8.0).area());
        assert_eq!(mesh.vertices[0].uv, egui::Pos2::ZERO);
        assert_eq!(mesh.vertices[3].uv, egui::pos2(8.0 / 64.0, 8.0 / 64.0));
        assert_eq!(
            mesh.vertices[3].pos - mesh.vertices[0].pos,
            egui::vec2(8.0, 8.0)
        );

        let tiny = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(12.0, 10.0));
        let mesh = image_mesh(tiny, &texture, &image, Color32::WHITE).unwrap();
        assert!(mesh.vertices.iter().all(|vertex| tiny.contains(vertex.pos)));
        drop(texture);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
    }

    #[test]
    fn source_region_crops_and_rejects_regions_outside_the_texture() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "region",
            ColorImage::filled([64, 64], Color32::WHITE),
            TextureOptions::LINEAR,
        );
        let mut image = BackgroundImageTheme {
            source_region: Some([16, 8, 17, 17]),
            scaling: BackgroundImageScaling::Original,
            ..Default::default()
        };
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 100.0));
        let mesh = image_mesh(viewport, &texture, &image, Color32::WHITE).unwrap();
        assert_eq!(mesh.calc_bounds().size(), egui::vec2(17.0, 17.0));
        assert_eq!(mesh.vertices[0].uv, egui::pos2(16.0 / 64.0, 8.0 / 64.0));
        assert_eq!(mesh.vertices[3].uv, egui::pos2(33.0 / 64.0, 25.0 / 64.0));
        image.source_region = Some([63, 0, 17, 17]);
        assert!(image_mesh(viewport, &texture, &image, Color32::WHITE).is_err());
        image.source_region = None;
        image.frame_border = 32;
        assert!(image_mesh(viewport, &texture, &image, Color32::WHITE).is_err());
        drop(texture);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
    }

    #[test]
    fn scaling_and_corner_placement_preserve_geometry() {
        let viewport = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 100.0));
        let mut image = BackgroundImageTheme::default();
        let source = egui::vec2(100.0, 100.0);
        assert_eq!(
            image_rect(viewport, source, &image).size(),
            egui::vec2(200.0, 200.0)
        );

        image.scaling = BackgroundImageScaling::Contain;
        image.position = BackgroundImagePosition::BottomRight;
        let rect = image_rect(viewport, source, &image);
        assert_eq!(rect.size(), source);
        assert_eq!(rect.right_bottom(), viewport.right_bottom());

        image.scaling = BackgroundImageScaling::Original;
        assert_eq!(
            image_rect(viewport, egui::vec2(64.0, 64.0), &image).size(),
            egui::vec2(64.0, 64.0)
        );

        image.scaling = BackgroundImageScaling::Stretch;
        assert_eq!(image_rect(viewport, source, &image), viewport);
    }

    #[test]
    fn png_retains_transparency() {
        let path = std::env::temp_dir().join(format!("kosk-alpha-{}.png", std::process::id()));
        let mut pixels = image::RgbaImage::new(2, 1);
        pixels.put_pixel(1, 0, image::Rgba([80, 100, 120, 255]));
        pixels.save(&path).unwrap();
        let image = load_image(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(image.size, [2, 1]);
        assert!(image.pixels.iter().any(|pixel| pixel.a() == 0));
        assert!(image.pixels.iter().any(|pixel| pixel.a() == 255));
    }

    #[test]
    fn svg_limits_and_alpha_are_preserved() {
        let image = load_svg(br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="2" height="2" fill="#e0a12a" fill-opacity="0.5"/></svg>"##).unwrap();
        assert_eq!(image.size, [4, 4]);
        assert_eq!(image.pixels[0].a(), 128);
        assert_eq!(image.pixels[15].a(), 0);
        assert!(
            load_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="4097" height="1"/>"#)
                .is_err()
        );
    }
}
