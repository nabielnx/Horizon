use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{HashMap, VecDeque},
    path::Path,
};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Imaging::*},
        System::Com::*,
    },
    core::*,
};
use windows_numerics::{Matrix3x2, Vector2};
pub(super) const DESIGN_CENTERED: u8 = 5;
#[cfg(test)]
#[path = "mascot_render.rs"]
mod mascot_render;
thread_local! {static FACTORY:OnceCell<ID2D1Factory>=const{OnceCell::new()};static FONTS:OnceCell<IDWriteFontCollection>=const{OnceCell::new()};static ICONS:RefCell<HashMap<&'static str,ID2D1Geometry>>=RefCell::new(HashMap::new());}
thread_local! { static EYE_SLIT: OnceCell<ID2D1Geometry> = const { OnceCell::new() }; }
unsafe fn factory() -> Result<ID2D1Factory> {
    FACTORY.with(|cell| {
        if let Some(f) = cell.get() {
            return Ok(f.clone());
        }
        let f: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let _ = cell.set(f.clone());
        Ok(f)
    })
}
unsafe fn fonts(write: &IDWriteFactory) -> Result<IDWriteFontCollection> {
    FONTS.with(|cell| {
        if let Some(f) = cell.get() {
            return Ok(f.clone());
        }
        unsafe {
            let advanced: IDWriteFactory5 = write.cast()?;
            let builder = advanced.CreateFontSetBuilder()?;
            let root = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("fonts")))
                .filter(|p| p.exists())
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts"));
            for name in ["Medium", "SemiBold", "Bold"] {
                let path = root.join(format!("PlusJakartaSans-{name}.ttf"));
                let file = write.CreateFontFileReference(
                    &HSTRING::from(path.to_string_lossy().as_ref()),
                    None,
                )?;
                builder.AddFontFile(&file)?;
            }
            for name in ["Regular", "SemiBold", "Bold", "ExtraLight"] {
                let path = root.join(format!("Poppins-{name}.ttf"));
                let file = write.CreateFontFileReference(
                    &HSTRING::from(path.to_string_lossy().as_ref()),
                    None,
                )?;
                builder.AddFontFile(&file)?;
            }
            let collection: IDWriteFontCollection = advanced
                .CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?
                .cast()?;
            let _ = cell.set(collection.clone());
            Ok(collection)
        }
    })
}
struct KaraokeLayout {
    text: String,
    width: u32,
    style: (u16, u16, u8),
    scroll: bool,
    layout: IDWriteTextLayout,
}
#[derive(Clone, Copy, PartialEq)]
pub struct ShellOutline {
    pub floating: f32,
    pub width: f32,
    pub height: f32,
    pub shape: f32,
    pub radius: f32,
    pub offset: f32,
}
pub struct Canvas {
    target: ID2D1RenderTarget,
    write: IDWriteFactory,
    fonts: IDWriteFontCollection,
    formats: RefCell<HashMap<(u16, bool, u8), IDWriteTextFormat>>,
    design_formats: RefCell<HashMap<(u16, u16, u8), IDWriteTextFormat>>,
    karaoke_layout: RefCell<VecDeque<KaraokeLayout>>,
    #[allow(clippy::type_complexity)]
    design_measurements: RefCell<std::collections::VecDeque<(String, u16, u16, u16, u8, f32, f32)>>,
    design_assets: RefCell<HashMap<&'static str, ID2D1Bitmap>>,
    lyric_glow_brushes: RefCell<Option<([u32; 3], [ID2D1RadialGradientBrush; 3])>>,
    eye_glass: OnceCell<ID2D1LinearGradientBrush>,
    #[cfg(test)]
    mascot_art: RefCell<Option<mascot_render::Art>>,
    bitmap: Option<IWICBitmap>,
    dpi: crate::dpi::Scale,
    width: u32,
    height: u32,
    presented: Cell<bool>,
    cover_cache: RefCell<Option<(String, ID2D1Bitmap)>>,
    preview_cache: RefCell<Option<(String, ID2D1Bitmap)>>,
    file_icons: RefCell<std::collections::VecDeque<(String, ID2D1Bitmap)>>,
    solid_brush: ID2D1SolidColorBrush,
    stroke: ID2D1StrokeStyle,
    glass_motion: Cell<bool>,
    icon_scale: Cell<f32>,
    interaction_pressed: Cell<bool>,
    interaction_pointer: Cell<(f32, f32)>,
    shell_mask: RefCell<Option<(ShellOutline, ID2D1Geometry)>>,
    shell_layer: Cell<bool>,
    #[allow(clippy::type_complexity)]
    glass_states: RefCell<HashMap<(i32, i32, i32, i32, u8), crate::motion::Tween>>,
}
impl Canvas {
    pub fn height(&self) -> u32 {
        self.height
    }
    pub unsafe fn resize_shell(&mut self, width: u32, height: u32) -> Result<()> {
        // Width changes keep the existing recreation path because they change layout.
        let previous = unsafe { self.target.GetPixelSize() };
        if width != previous.width || height == 0 {
            return Err(Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "Shell layout size changed",
            ));
        }
        if height != previous.height {
            unsafe {
                self.target
                    .cast::<ID2D1HwndRenderTarget>()?
                    .Resize(&D2D_SIZE_U { width, height })?;
            }
            self.height = self.dpi.dip(height as i32) as u32;
            self.presented.set(false);
            if self.height == crate::layout::COMPACT_BUFFER_HEIGHT {
                self.design_assets.borrow_mut().clear();
                self.karaoke_layout.borrow_mut().clear();
                self.design_measurements.borrow_mut().clear();
                self.lyric_glow_brushes.borrow_mut().take();
            }
        }
        Ok(())
    }
    pub unsafe fn new(
        hwnd: HWND,
        width: u32,
        height: u32,
        alpha: bool,
        dpi: crate::dpi::Scale,
    ) -> Result<Self> {
        unsafe {
            let target = factory()?
                .CreateHwndRenderTarget(
                    &D2D1_RENDER_TARGET_PROPERTIES {
                        r#type: if std::env::args().any(|a| a == "--hardware-render") {
                            D2D1_RENDER_TARGET_TYPE_DEFAULT
                        } else {
                            D2D1_RENDER_TARGET_TYPE_SOFTWARE
                        },
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: if alpha {
                                D2D1_ALPHA_MODE_PREMULTIPLIED
                            } else {
                                D2D1_ALPHA_MODE_IGNORE
                            },
                        },
                        dpiX: dpi.dpi(),
                        dpiY: dpi.dpi(),
                        ..Default::default()
                    },
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd,
                        pixelSize: D2D_SIZE_U { width, height },
                        presentOptions: D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS,
                    },
                )?
                .cast()?;
            Self::from_target(target, None, width, height, dpi)
        }
    }
    pub unsafe fn image(width: u32, height: u32) -> Result<Self> {
        unsafe { Self::image_at_dpi(width, height, crate::dpi::Scale::STANDARD) }
    }
    unsafe fn image_at_dpi(width: u32, height: u32, dpi: crate::dpi::Scale) -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let bitmap = wic.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnLoad,
            )?;
            let target = factory()?.CreateWicBitmapRenderTarget(
                &bitmap,
                &D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: dpi.dpi(),
                    dpiY: dpi.dpi(),
                    ..Default::default()
                },
            )?;
            Self::from_target(target, Some(bitmap), width, height, dpi)
        }
    }
    unsafe fn from_target(
        target: ID2D1RenderTarget,
        bitmap: Option<IWICBitmap>,
        width: u32,
        height: u32,
        dpi: crate::dpi::Scale,
    ) -> Result<Self> {
        unsafe {
            crate::trace("canvas: text factory");
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            crate::trace("canvas: fonts");
            let fonts = fonts(&write)?;
            crate::trace("canvas: ready");
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            // Seed the retained backing surface before any region is exposed or copied.
            target.BeginDraw();
            target.Clear(Some(&color(0x090a0c)));
            target.EndDraw(None, None)?;
            let solid_brush = target.CreateSolidColorBrush(&color(0xffffff), None)?;
            Ok(Self {
                target,
                write,
                fonts,
                formats: RefCell::new(HashMap::new()),
                design_formats: RefCell::new(HashMap::new()),
                karaoke_layout: RefCell::new(VecDeque::new()),
                design_measurements: RefCell::new(std::collections::VecDeque::new()),
                design_assets: RefCell::new(HashMap::new()),
                lyric_glow_brushes: RefCell::new(None),
                eye_glass: OnceCell::new(),
                #[cfg(test)]
                mascot_art: RefCell::new(None),
                presented: Cell::new(false),
                glass_motion: Cell::new(false),
                icon_scale: Cell::new(1.),
                glass_states: RefCell::new(HashMap::new()),
                interaction_pressed: Cell::new(false),
                interaction_pointer: Cell::new((-1., -1.)),
                shell_mask: RefCell::new(None),
                shell_layer: Cell::new(false),
                cover_cache: RefCell::new(None),
                preview_cache: RefCell::new(None),
                file_icons: RefCell::new(std::collections::VecDeque::new()),
                solid_brush,
                stroke: factory()?.CreateStrokeStyle(
                    &D2D1_STROKE_STYLE_PROPERTIES {
                        startCap: D2D1_CAP_STYLE_ROUND,
                        endCap: D2D1_CAP_STYLE_ROUND,
                        lineJoin: D2D1_LINE_JOIN_ROUND,
                        ..Default::default()
                    },
                    None,
                )?,
                bitmap,
                dpi,
                width: dpi.dip(width as i32) as u32,
                height: dpi.dip(height as i32) as u32,
            })
        }
    }
    unsafe fn brush(&self, c: u32) -> Result<ID2D1SolidColorBrush> {
        unsafe {
            self.solid_brush.SetColor(&color(c));
            Ok(self.solid_brush.clone())
        }
    }
    pub fn release_preview(&self) {
        self.preview_cache.borrow_mut().take();
        self.file_icons.borrow_mut().clear();
    }
    #[expect(dead_code, reason = "Legacy selected rect")]
    pub unsafe fn selected_rect(&self, x: f32, y: f32, w: f32, h: f32) {
        unsafe {
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(0x30ffffff),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(0x18ffffff),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: x, Y: y },
                    endPoint: Vector2 { X: x + w, Y: y + h },
                },
                None,
                &stops,
            ) {
                let radius = (w * 0.25).clamp(7., 10.);
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                );
            }
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Liquid glass rendering requires positioning, dimensions, radius, semantic accents, and interaction states"
    )]
    pub unsafe fn liquid_glass(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        accent: Option<u32>,
        hover: bool,
        active: bool,
    ) {
        unsafe {
            if let Ok(shadow_brush) = self.brush(0x35000000) {
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y + 1.2, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &shadow_brush,
                );
            }

            let bounds = (x, y, w, h);
            let light = self.hover_amount(bounds, hover);
            let selection = self.animate_value(bounds, 2, f32::from(active), crate::motion::STATE);
            let hover_fill = accent.map_or(0x343434, |accent| material_fill(Some(accent)));
            let base_color = crate::motion::mix(0x1b1b1b, hover_fill, light.max(selection));
            if let Ok(base_brush) = self.brush(base_color) {
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &base_brush,
                );
            }

            let top_sheen = (((21. + light * 11. + selection * 5.) as u32) << 24) | 0xffffff;

            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(top_sheen),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.42,
                        color: color(0x01ffffff),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(0x01ffffff),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: x, Y: y },
                    endPoint: Vector2 { X: x, Y: y + h },
                },
                None,
                &stops,
            ) {
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                );
            }

            let rim = |base: f32, hover_lift: f32, selected_lift: f32| {
                (((base + hover_lift * light + selected_lift * selection) as u32) << 24) | 0xffffff
            };
            let (rim_top, rim_mid, rim_bot) =
                (rim(48., 12., 4.), rim(16., 4., 2.), rim(6., 1., 1.));

            if let Ok(rim_stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(rim_top),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.40,
                        color: color(rim_mid),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(rim_bot),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(rim_brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 {
                        X: x + w * 0.15,
                        Y: y,
                    },
                    endPoint: Vector2 {
                        X: x + w * 0.85,
                        Y: y + h,
                    },
                },
                None,
                &rim_stops,
            ) {
                self.target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0),
                        radiusX: (radius - 0.5).max(1.0),
                        radiusY: (radius - 0.5).max(1.0),
                    },
                    &rim_brush,
                    1.0,
                    None,
                );
            }
        }
    }
    pub fn set_glass_motion(&self, enabled: bool) {
        self.glass_motion.set(enabled);
    }
    pub fn set_icon_scale(&self, scale: f32) -> f32 {
        self.icon_scale.replace(scale)
    }
    pub fn scaled_icon_bounds(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let scale = self.icon_scale.get();
        let inset = (1. - scale) / 2.;
        (x + w * inset, y + h * inset, w * scale, h * scale)
    }
    pub fn set_interaction_pressed(&self, pressed: bool) {
        self.interaction_pressed.set(pressed);
    }
    pub fn set_interaction_pointer(&self, point: (i32, i32)) {
        self.interaction_pointer
            .set((point.0 as f32, point.1 as f32));
    }
    pub fn animate_value(
        &self,
        b: (f32, f32, f32, f32),
        channel: u8,
        target: f32,
        duration: f32,
    ) -> f32 {
        let key = (
            b.0.round() as i32,
            b.1.round() as i32,
            b.2.round() as i32,
            b.3.round() as i32,
            channel,
        );
        let now = std::time::Instant::now();
        let mut states = self.glass_states.borrow_mut();
        if states.len() >= 512 && !states.contains_key(&key) {
            states.retain(|_, value| value.active(now));
            if states.len() >= 512 {
                states.clear();
            }
        }
        states
            .entry(key)
            .or_insert_with(|| crate::motion::Tween::new(target, now))
            .retarget(target, duration, self.glass_motion.get(), now)
    }
    pub fn hover_amount(&self, bounds: (f32, f32, f32, f32), hover: bool) -> f32 {
        self.animate_value(bounds, 0, f32::from(hover), crate::motion::HOVER)
    }
    pub fn pulse(&self, b: (i32, i32, i32, i32)) {
        if self.glass_motion.get() {
            self.glass_states.borrow_mut().insert(
                (b.0, b.1, b.2, b.3, 1),
                crate::motion::Tween::new(1., std::time::Instant::now()),
            );
        }
    }
    pub fn glass_animating(&self) -> bool {
        self.glass_motion.get()
            && self
                .glass_states
                .borrow()
                .values()
                .any(|value| value.active(std::time::Instant::now()))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Material bounds and interaction state"
    )]
    pub unsafe fn control_glass(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        accent: Option<u32>,
        hover: bool,
        active: bool,
    ) -> f32 {
        unsafe {
            let bounds = (x, y, w, h);
            let light = self.hover_amount(bounds, hover);
            let press = self.animate_value(
                bounds,
                1,
                f32::from(hover && self.interaction_pressed.get()),
                crate::motion::PRESS,
            );
            let selection = self.animate_value(bounds, 2, f32::from(active), crate::motion::STATE);
            let offset = if self.glass_motion.get() {
                -light + press * 2.
            } else {
                0.
            };
            let y = y + offset;
            let tint = material_fill(accent);
            let base = {
                [16_u32, 8, 0]
                    .into_iter()
                    .map(|shift| {
                        let lift = light * 13. + selection * 6. - press * 5.;
                        ((((tint >> shift) & 255) as f32 + lift).min(255.) as u32) << shift
                    })
                    .sum()
            };
            self.rect(x, y, w, h, radius, base);
            self.glass_edge(x, y, w, h, radius, light);
            if selection > 0.001 {
                // Keep the surface dark; a small opaque marker carries the page colour.
                let marker_w = (w * 0.24).clamp(8., 18.) * selection;
                self.rect(
                    x + (w - marker_w) / 2.,
                    y + h - 4.,
                    marker_w,
                    2.,
                    1.,
                    material_accent(accent),
                );
            }
            offset
        }
    }
    pub unsafe fn glass_button(
        &self,
        bounds: (f32, f32, f32, f32),
        radius: f32,
        hovered: bool,
    ) -> f32 {
        unsafe { self.glass_content(bounds, radius, hovered, None, |_| {}) }
    }
    pub unsafe fn glass_content(
        &self,
        bounds: (f32, f32, f32, f32),
        radius: f32,
        hovered: bool,
        active: Option<bool>,
        content: impl FnOnce((f32, f32, f32, f32)),
    ) -> f32 {
        unsafe {
            let light = self.hover_amount(bounds, hovered);
            let press = self.animate_value(
                bounds,
                1,
                f32::from(hovered && self.interaction_pressed.get()),
                crate::motion::PRESS,
            );
            let selection = active.map_or(0., |active| {
                self.animate_value(bounds, 2, f32::from(active), crate::motion::STATE)
            });
            let dy = if self.glass_motion.get() {
                -light + press * 2.
            } else {
                0.
            };
            let (x, y, w, h) = bounds;
            self.rect(
                x,
                y + dy,
                w,
                h,
                radius,
                crate::motion::mix(
                    crate::motion::mix(0x202022, 0x303033, (light - press * 0.25).max(0.)),
                    0x424244,
                    selection * 0.95,
                ),
            );
            content((x, y + dy, w, h));
            self.glass_finish((x, y + dy, w, h), radius, light);
            if selection > 0.001 {
                let marker = w.min(50.) * 0.26 * selection;
                self.rect(
                    x + (w - marker) / 2.,
                    y + dy + h - 5.,
                    marker,
                    2.,
                    1.,
                    0xe9e9ec,
                );
            }
            dy
        }
    }
    pub unsafe fn glass_panel(
        &self,
        bounds: (f32, f32, f32, f32),
        radius: f32,
        fill: u32,
        hover: f32,
    ) {
        unsafe {
            self.rect(bounds.0, bounds.1, bounds.2, bounds.3, radius, fill);
            self.glass_finish(bounds, radius, hover);
        }
    }
    unsafe fn glass_finish(&self, bounds: (f32, f32, f32, f32), radius: f32, hover: f32) {
        unsafe {
            let (x, y, w, h) = bounds;
            let mut transform = Matrix3x2::default();
            self.target.GetTransform(&mut transform);
            let pointer = self.interaction_pointer.get();
            let local_x = (pointer.0 - transform.M31) / transform.M11.max(0.001);
            let local_y = pointer.1 - transform.M32;
            let follow = if local_x >= x && local_x <= x + w && local_y >= y && local_y <= y + h {
                (local_x - x - w * 0.5) * hover * 0.6
            } else {
                0.
            };
            // A face reflection is composited over an opaque base. It does not
            // expose wallpaper or include another outline around the icon.
            let face_alpha = (16. + hover.clamp(0., 1.) * 12.) as u32;
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color((face_alpha << 24) | 0xffffff),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.55,
                        color: color(0x01ffffff),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(0x04ffffff),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 {
                        X: x + follow,
                        Y: y,
                    },
                    endPoint: Vector2 {
                        X: x + w * 0.25 + follow,
                        Y: y + h,
                    },
                },
                None,
                &stops,
            ) {
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                );
            }
            self.glass_edge(x, y, w, h, radius, hover);
        }
    }
    pub unsafe fn glass_edge(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, hover: f32) {
        unsafe {
            // One contour, with a stronger upper reflection fading toward the
            // lower edge. Hover changes light, never the corner geometry.
            {
                let alpha = (104. + hover.clamp(0., 1.) * 24.) as u32;
                let stroke = 1.;
                let colors = [(alpha << 24) | 0xffffff, 0x18ffffff, 0x28ffffff];
                let inset = stroke / 2.;
                let stops = self.target.CreateGradientStopCollection(
                    &[
                        D2D1_GRADIENT_STOP {
                            position: 0.,
                            color: color(colors[0]),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 0.42,
                            color: color(colors[1]),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.,
                            color: color(colors[2]),
                        },
                    ],
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                );
                if let Ok(stops) = stops
                    && let Ok(brush) = self.target.CreateLinearGradientBrush(
                        &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: Vector2 {
                                X: x + w * 0.35,
                                Y: y,
                            },
                            endPoint: Vector2 {
                                X: x + w * 0.5,
                                Y: y + h,
                            },
                        },
                        None,
                        &stops,
                    )
                {
                    let shape = D2D1_ROUNDED_RECT {
                        rect: rect(x + inset, y + inset, w - inset * 2., h - inset * 2.),
                        radiusX: (radius - inset).max(1.),
                        radiusY: (radius - inset).max(1.),
                    };
                    self.target
                        .DrawRoundedRectangle(&shape, &brush, stroke, None);
                }
            }
        }
    }
    pub unsafe fn snapshot(&self) -> Result<ID2D1Bitmap> {
        if !self.presented.get() {
            return Err(Error::new(
                windows::Win32::Foundation::E_UNEXPECTED,
                "No frame has been presented",
            ));
        }
        unsafe {
            let bitmap = self.target.CreateBitmap(
                self.target.GetPixelSize(),
                None,
                0,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: self.target.GetPixelFormat(),
                    dpiX: self.dpi.dpi(),
                    dpiY: self.dpi.dpi(),
                },
            )?;
            bitmap.CopyFromRenderTarget(None, &self.target, None)?;
            Ok(bitmap)
        }
    }
    pub fn has_frame(&self) -> bool {
        self.presented.get()
    }
    pub unsafe fn overlay(
        &self,
        bitmap: &ID2D1Bitmap,
        opacity: f32,
        shell: bool,
        drift: f32,
        content_left: f32,
    ) {
        unsafe {
            let x = if shell { 0. } else { content_left };
            let y = if shell { 0. } else { 14. };
            let size = bitmap.GetSize();
            let width = (self.width as f32).min(size.width) - x;
            let height = (self.height as f32).min(size.height) - y;
            if width <= 0. || height <= 0. {
                return;
            }
            let bounds = rect(x, y, width, height);
            self.target
                .PushAxisAlignedClip(&bounds, D2D1_ANTIALIAS_MODE_ALIASED);
            let destination = rect(x, y + drift, width, height);
            self.target.DrawBitmap(
                bitmap,
                Some(&destination),
                opacity,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                Some(&bounds),
            );
            self.target.PopAxisAlignedClip();
        }
    }
    pub unsafe fn clip_scroll(&self, x: f32, y: f32, w: f32, h: f32, scroll: f32) {
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect(x, y, w, h), D2D1_ANTIALIAS_MODE_ALIASED);
            self.target.SetTransform(&Matrix3x2 {
                M11: 1.,
                M22: 1.,
                M32: -scroll,
                ..Default::default()
            });
        }
    }
    pub unsafe fn end_clip(&self) {
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: 1.,
                M22: 1.,
                ..Default::default()
            });
            self.target.PopAxisAlignedClip();
        }
    }
    pub unsafe fn begin_palette(&self, colors: [u32; 3]) {
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(0x17191c)));
            self.fill_palette(colors, self.height);
        }
    }
    pub unsafe fn begin_shell_palette(
        &self,
        colors: [u32; 3],
        outline: ShellOutline,
    ) -> Result<()> {
        unsafe {
            let mut cache = self.shell_mask.borrow_mut();
            if cache.as_ref().is_none_or(|(key, _)| *key != outline) {
                let geometry = factory()?.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                let points = crate::layout::contour_styled(
                    outline.width,
                    outline.height,
                    outline.shape,
                    outline.radius,
                    outline.floating,
                );
                sink.BeginFigure(
                    Vector2 {
                        X: points[0].0 + outline.offset,
                        Y: points[0].1,
                    },
                    D2D1_FIGURE_BEGIN_FILLED,
                );
                for &(x, y) in &points[1..] {
                    sink.AddLine(Vector2 {
                        X: x + outline.offset,
                        Y: y,
                    });
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                *cache = Some((outline, geometry.cast()?));
            }
            let mut parameters = D2D1_LAYER_PARAMETERS {
                contentBounds: rect(
                    outline.offset - 1.,
                    0.,
                    outline.width + 2.,
                    outline.height + 1.,
                ),
                geometricMask: std::mem::ManuallyDrop::new(Some(cache.as_ref().unwrap().1.clone())),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2 {
                    M11: 1.,
                    M22: 1.,
                    ..Default::default()
                },
                opacity: 1.,
                ..Default::default()
            };
            self.target.BeginDraw();
            self.target.Clear(Some(&D2D1_COLOR_F::default()));
            self.target.PushLayer(&parameters, None);
            std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
            self.shell_layer.set(true);
            let gradient_height = if self.height == crate::layout::COMPACT_BUFFER_HEIGHT {
                crate::host_height() as u32
            } else {
                self.height
            };
            self.fill_palette(colors, gradient_height);
            Ok(())
        }
    }
    unsafe fn fill_palette(&self, colors: [u32; 3], gradient_height: u32) {
        unsafe {
            self.rect(0., 0., self.width as f32, self.height as f32, 0., 0x17191c);
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(colors[0]),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.5,
                        color: color(colors[1]),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(colors[2]),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: 0., Y: 0. },
                    endPoint: Vector2 {
                        X: self.width as f32 * 0.6,
                        Y: gradient_height as f32,
                    },
                },
                None,
                &stops,
            ) {
                self.target
                    .FillRectangle(&rect(0., 0., self.width as f32, self.height as f32), &brush);
            }
        }
    }
    pub unsafe fn ambient_glow(&self, width: f32, height: f32, offset: f32, c1: u32, c2: u32) {
        unsafe {
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.0,
                        color: D2D1_COLOR_F {
                            r: 0.,
                            g: 0.,
                            b: 0.,
                            a: 0.,
                        },
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.28,
                        color: D2D1_COLOR_F {
                            r: 0.,
                            g: 0.,
                            b: 0.,
                            a: 0.,
                        },
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.72,
                        color: D2D1_COLOR_F {
                            r: ((c1 >> 16) & 255) as f32 / 255.,
                            g: ((c1 >> 8) & 255) as f32 / 255.,
                            b: (c1 & 255) as f32 / 255.,
                            a: 0.28,
                        },
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.0,
                        color: D2D1_COLOR_F {
                            r: ((c2 >> 16) & 255) as f32 / 255.,
                            g: ((c2 >> 8) & 255) as f32 / 255.,
                            b: (c2 & 255) as f32 / 255.,
                            a: 0.45,
                        },
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 {
                        X: offset + width * 0.5,
                        Y: 0.,
                    },
                    endPoint: Vector2 {
                        X: offset + width * 0.5,
                        Y: height,
                    },
                },
                None,
                &stops,
            ) {
                self.target
                    .FillRectangle(&rect(offset, 0., width, height), &brush);
            }
        }
    }
    pub unsafe fn design_glow(&self, height: f32, tone: u32) {
        unsafe {
            self.design_glow_at((0., 0., 591., height), tone);
        }
    }
    pub unsafe fn design_glow_at(&self, bounds: (f32, f32, f32, f32), tone: u32) {
        unsafe {
            let (x, y, width, height) = bounds;
            let mut rgb = color(tone);
            rgb.r = ((rgb.r * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
            rgb.g = ((rgb.g * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
            rgb.b = ((rgb.b * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
            let peak = rgb.r.max(rgb.g).max(rgb.b);
            let gain = if peak > 0. { (0.43 / peak).max(1.) } else { 1. };
            rgb.r = (rgb.r * gain).min(1.);
            rgb.g = (rgb.g * gain).min(1.);
            rgb.b = (rgb.b * gain).min(1.);
            let luminance = rgb.r * 0.2126 + rgb.g * 0.7152 + rgb.b * 0.0722;
            rgb.a = if height < 150. { 0.76 } else { 0.65 } * (0.48 / luminance.max(0.001)).min(1.);
            let mut middle = rgb;
            middle.a *= 0.4;
            let clear = D2D1_COLOR_F { a: 0., ..rgb };
            let center = Vector2 {
                X: x + 35_f32.min(width * 0.08),
                Y: y + 35_f32.min(height * 0.4),
            };
            let radius_x = width * 0.65;
            let radius_y = height * 1.1;
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: rgb,
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.48,
                        color: middle,
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: clear,
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center,
                    gradientOriginOffset: Vector2 { X: 0., Y: 0. },
                    radiusX: radius_x,
                    radiusY: radius_y,
                },
                None,
                &stops,
            ) {
                // Complete radial falloff; only the outer shell clips it.
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: center,
                        radiusX: radius_x,
                        radiusY: radius_y,
                    },
                    &brush,
                );
            }
        }
    }
    pub unsafe fn lyric_glow(
        &self,
        bounds: (f32, f32, f32, f32),
        palette: [u32; 3],
        seconds: f64,
        seed: u64,
        opacity: f32,
    ) {
        unsafe {
            let mut cache = self.lyric_glow_brushes.borrow_mut();
            if cache.as_ref().is_none_or(|(colors, _)| *colors != palette) {
                let create = |tone| -> Result<ID2D1RadialGradientBrush> {
                    let mut rgb = color(tone);
                    rgb.r = ((rgb.r * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
                    rgb.g = ((rgb.g * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
                    rgb.b = ((rgb.b * 255. - 8.) / 0.34).clamp(0., 255.) / 255.;
                    // Preserve hue while bounding overlapping glow brightness
                    // underneath white lyrics, even for a near-white cover.
                    let gain = 0.55 / rgb.r.max(rgb.g).max(rgb.b).max(0.001);
                    rgb.r *= gain;
                    rgb.g *= gain;
                    rgb.b *= gain;
                    rgb.a = 1.;
                    let stops = self.target.CreateGradientStopCollection(
                        &[
                            D2D1_GRADIENT_STOP {
                                position: 0.,
                                color: rgb,
                            },
                            D2D1_GRADIENT_STOP {
                                position: 0.45,
                                color: D2D1_COLOR_F { a: 0.32, ..rgb },
                            },
                            D2D1_GRADIENT_STOP {
                                position: 1.,
                                color: D2D1_COLOR_F { a: 0., ..rgb },
                            },
                        ],
                        D2D1_GAMMA_2_2,
                        D2D1_EXTEND_MODE_CLAMP,
                    )?;
                    self.target.CreateRadialGradientBrush(
                        &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                            radiusX: 1.,
                            radiusY: 1.,
                            ..Default::default()
                        },
                        None,
                        &stops,
                    )
                };
                let brushes = (|| -> Result<_> {
                    Ok([
                        create(palette[0])?,
                        create(palette[1])?,
                        create(palette[2])?,
                    ])
                })();
                let Ok(brushes) = brushes else {
                    return;
                };
                *cache = Some((palette, brushes));
            }
            let (x, y, width, height) = bounds;
            let blobs = crate::motion::glow_blobs(seconds, seed);
            for (brush, blob) in cache.as_ref().unwrap().1.iter().zip(blobs) {
                let center = Vector2 {
                    X: x + width * blob.x,
                    Y: y + height * blob.y,
                };
                let radius_x = width * blob.radius_x;
                let radius_y = height * blob.radius_y;
                brush.SetCenter(center);
                brush.SetRadiusX(radius_x);
                brush.SetRadiusY(radius_y);
                brush.SetOpacity(blob.opacity * opacity);
                // Each ellipse fades to zero at its edge; no rectangular patch
                // or hard clipping boundary separates the two music columns.
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: center,
                        radiusX: radius_x,
                        radiusY: radius_y,
                    },
                    brush,
                );
            }
        }
    }
    pub unsafe fn cover(&self, key: &str, bytes: &[u8], x: f32, y: f32, size: f32) -> Result<()> {
        unsafe { self.cover_rect(key, bytes, (x, y, size, size), 10.) }
    }
    pub unsafe fn cover_rect(
        &self,
        key: &str,
        bytes: &[u8],
        b: (f32, f32, f32, f32),
        radius: f32,
    ) -> Result<()> {
        unsafe {
            let (x, y, w, h) = b;
            let mut cache = self.cover_cache.borrow_mut();
            if cache.as_ref().is_none_or(|(old, _)| old != key) {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let source = crate::artwork::source(bytes, 160)?;
                let bitmap = self.target.CreateBitmapFromWicBitmap(&source, None)?;
                *cache = Some((key.into(), bitmap));
            }
            if let Some((_, bitmap)) = &*cache {
                let mask: ID2D1Geometry = factory()?
                    .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    })?
                    .cast()?;
                let mut parameters = D2D1_LAYER_PARAMETERS {
                    contentBounds: rect(x, y, w, h),
                    geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2 {
                        M11: 1.,
                        M22: 1.,
                        ..Default::default()
                    },
                    opacity: 1.,
                    ..Default::default()
                };
                self.target.PushLayer(&parameters, None);
                self.target.DrawBitmap(
                    bitmap,
                    Some(&rect(x, y, w, h)),
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
                self.target.PopLayer();
                std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
            }
            Ok(())
        }
    }
    unsafe fn preview_bitmap(&self, image: &crate::artwork::PreviewImage) -> Result<ID2D1Bitmap> {
        unsafe {
            self.target.CreateBitmap(
                D2D_SIZE_U {
                    width: image.width,
                    height: image.height,
                },
                Some(image.pixels.as_ptr().cast()),
                image.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.,
                    dpiY: 96.,
                },
            )
        }
    }
    pub unsafe fn attachment_thumbnail(
        &self,
        key: &str,
        bytes: &[u8],
        (x, y, size): (f32, f32, f32),
    ) -> Result<()> {
        unsafe {
            let mut cache = self.file_icons.borrow_mut();
            if !cache.iter().any(|(name, _)| name == key) {
                let source = crate::artwork::source(bytes, 44)?;
                let bitmap = self.target.CreateBitmapFromWicBitmap(&source, None)?;
                if cache.len() >= 12 {
                    cache.pop_front();
                }
                cache.push_back((key.into(), bitmap));
            }
            if let Some((_, bitmap)) = cache.iter().find(|(name, _)| name == key) {
                self.target.DrawBitmap(
                    bitmap,
                    Some(&rect(x, y, size, size)),
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
            Ok(())
        }
    }
    pub unsafe fn file_icon(
        &self,
        path: &str,
        image: &crate::artwork::PreviewImage,
        (x, y, size): (f32, f32, f32),
    ) -> Result<()> {
        unsafe {
            let mut cache = self.file_icons.borrow_mut();
            let bitmap = if let Some((_, bitmap)) = cache.iter().find(|(key, _)| key == path) {
                bitmap
            } else {
                let bitmap = self.preview_bitmap(image)?;
                if cache.len() >= 12 {
                    cache.pop_front();
                }
                cache.push_back((path.into(), bitmap));
                &cache.back().unwrap().1
            };
            self.target.DrawBitmap(
                bitmap,
                Some(&rect(x, y, size, size)),
                1.,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
            Ok(())
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn image_preview(
        &self,
        key: &str,
        image: &crate::artwork::PreviewImage,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
    ) -> Result<()> {
        unsafe {
            let mut cache = self.preview_cache.borrow_mut();
            if cache.as_ref().is_none_or(|(old, _)| old != key) {
                *cache = Some((key.into(), self.preview_bitmap(image)?));
            }
            if let Some((_, bitmap)) = &*cache {
                let mask: ID2D1Geometry = factory()?
                    .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: radius,
                        radiusY: radius,
                    })?
                    .cast()?;
                let mut parameters = D2D1_LAYER_PARAMETERS {
                    contentBounds: rect(x, y, w, h),
                    geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2 {
                        M11: 1.,
                        M22: 1.,
                        ..Default::default()
                    },
                    opacity: 1.,
                    ..Default::default()
                };
                self.target.PushLayer(&parameters, None);
                let scale = (w / image.width as f32).min(h / image.height as f32);
                let iw = image.width as f32 * scale;
                let ih = image.height as f32 * scale;
                self.target.DrawBitmap(
                    bitmap,
                    Some(&rect(x + (w - iw) / 2., y + (h - ih) / 2., iw, ih)),
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
                self.target.PopLayer();
                std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
            }
            Ok(())
        }
    }
    pub unsafe fn wave(
        &self,
        x: f32,
        y: f32,
        width: f32,
        fraction: f32,
        seconds: f64,
        moving: bool,
    ) {
        unsafe {
            let inset = 7.;
            let length = (width - inset * 2.).max(0.);
            let end = length * fraction.clamp(0., 1.);
            let span = (end - 8.).max(0.);
            let base = y + 23.;
            self.rect(x + inset, base, length, 5., 2.5, 0x50565b);
            if span > 0. {
                for layer in 0..2 {
                    let geometry = factory().and_then(|f| f.CreatePathGeometry());
                    if let Ok(path) = geometry
                        && let Ok(sink) = path.Open()
                    {
                        sink.BeginFigure(
                            Vector2 {
                                X: x + inset,
                                Y: base + 5.,
                            },
                            D2D1_FIGURE_BEGIN_FILLED,
                        );
                        let count = (span / 2.).ceil().max(2.) as usize;
                        let phase = if moving { seconds * 3.6 } else { 0. };
                        for i in 0..=count {
                            let px = span * i as f32 / count as f32;
                            let h = wave_height(px as f64, span as f64, phase, layer == 0) as f32;
                            sink.AddLine(Vector2 {
                                X: x + inset + px,
                                Y: base - h,
                            });
                        }
                        sink.AddLine(Vector2 {
                            X: x + inset + span,
                            Y: base + 5.,
                        });
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        if sink.Close().is_ok()
                            && let Ok(brush) = self.target.CreateSolidColorBrush(
                                &D2D1_COLOR_F {
                                    r: 0.84,
                                    g: 0.86,
                                    b: 0.88,
                                    a: if layer == 0 { 0.35 } else { 0.7 },
                                },
                                None,
                            )
                        {
                            self.target.FillGeometry(&path, &brush, None);
                        }
                    }
                }
            }
            self.rect(x + inset, base, end, 5., 2.5, 0xbcc5cc);
            self.rect(x + inset + end - 6., base - 3.5, 12., 12., 6., 0xe7edf2);
        }
    }
    pub unsafe fn mini_wave(
        &self,
        x: f32,
        y: f32,
        width: f32,
        fraction: f32,
        seconds: f64,
        moving: bool,
    ) {
        unsafe {
            self.mini_wave_tinted((x, y, width, 4.2), fraction, seconds, moving, None, 2.5);
        }
    }
    pub unsafe fn mini_wave_tinted(
        &self,
        bounds: (f32, f32, f32, f32),
        fraction: f32,
        seconds: f64,
        moving: bool,
        tint: Option<[u32; 2]>,
        rail_h: f32,
    ) {
        unsafe {
            let (x, y, width, amplitude) = bounds;
            let inset = 2.;
            let length = (width - inset * 2.).max(1.);
            let played_end = inset + length * fraction.clamp(0., 1.);
            // Coloured idle timelines must end at the same position as the
            // playhead, including the opening seconds of a track.
            let played_span = played_end - inset;
            let active_span = if tint.is_some() {
                played_span
            } else {
                played_span.max(if moving || fraction > 0. { 24. } else { 0. })
            };
            let wave_end = (inset + active_span).min(inset + length);
            let rail_y = y + 5.;
            let wave_colors = tint.map(|[primary, secondary]| {
                let front = wave_front_color(primary);
                let mut back = crate::motion::mix(secondary, 0xffffff, 0.12);
                let distinct = [0, 8, 16]
                    .into_iter()
                    .any(|shift| ((front >> shift) & 255).abs_diff((back >> shift) & 255) >= 56);
                if !distinct {
                    back = crate::motion::mix(front, 0x101217, 0.4);
                }
                [back, front]
            });

            // A zero rail thickness draws only the wave, flush to its baseline.
            if rail_h > 0. {
                self.rect(
                    x + inset,
                    rail_y,
                    length,
                    rail_h,
                    rail_h / 2.,
                    tint.map_or(0x485057, |[primary, _]| {
                        crate::motion::mix(primary, 0x14161a, 0.68)
                    }),
                );
            }

            // 2. Liquid wave animation
            if active_span > if rail_h > 0. { 4. } else { 0. } {
                let amplitude = if rail_h > 0. {
                    amplitude
                } else {
                    amplitude * (active_span / 16.).min(1.)
                };
                let samples = (active_span / 2.).ceil().max(16.) as usize;
                let phase = if moving || tint.is_some() {
                    seconds * 3.6
                } else {
                    0.
                };
                let span = f64::from(active_span);
                let (flat, taper) = if tint.is_some() {
                    // Give the playhead and the start of the rail a flat run-in.
                    // Scale it down on short fills to leave room for the crests.
                    (
                        if rail_h > 0. {
                            (f64::from(rail_h) + 2.).min(span * 0.08)
                        } else {
                            0.
                        },
                        (12. + f64::from(amplitude)).min(span * 0.32),
                    )
                } else {
                    (0., 8.)
                };

                for layer in 0..2 {
                    let back = layer == 0;
                    // Long, slow swells in back cross the shorter, quicker
                    // crests in front. Both use the same tapered endpoints.
                    let (baseline, wavelengths, speeds, weights, offset) =
                        match (tint.is_some(), back) {
                            (true, true) => (
                                0.55,
                                [112., 186., 278.],
                                [0.35, 0.22, 0.15],
                                [0.25, 0.13, 0.07],
                                1.6,
                            ),
                            (true, false) => (
                                0.47,
                                [51., 87., 145.],
                                [0.99, 0.61, 0.30],
                                [0.26, 0.13, 0.08],
                                0.15,
                            ),
                            (false, true) => (
                                0.54 * 0.88,
                                [85., 135., 200.],
                                [0.65, 0.40, 0.22],
                                [0.25 * 0.88, 0.13 * 0.88, 0.08 * 0.88],
                                1.3,
                            ),
                            (false, false) => (
                                0.54,
                                [70., 110., 160.],
                                [0.75, 0.48, 0.28],
                                [0.25, 0.13, 0.08],
                                0.,
                            ),
                        };
                    let scale = if tint.is_some() {
                        (f64::from(length) / 250.).clamp(0.65, 1.)
                    } else {
                        1.
                    };
                    let [w1, w2, w3] = wavelengths.map(|w| std::f64::consts::TAU / (w * scale));
                    let [p1, p2, p3] = [
                        phase * speeds[0] + offset,
                        phase * speeds[1] + offset * 0.6 - 1.1,
                        phase * speeds[2] - 2.2,
                    ];
                    let geometry = factory().and_then(|f| f.CreatePathGeometry());
                    if let Ok(path) = geometry
                        && let Ok(sink) = path.Open()
                    {
                        sink.BeginFigure(
                            Vector2 {
                                X: x + inset,
                                Y: rail_y + rail_h,
                            },
                            D2D1_FIGURE_BEGIN_FILLED,
                        );
                        for i in 0..=samples {
                            let cur_x = inset + active_span * i as f32 / samples as f32;
                            let u = (cur_x - inset) as f64;
                            let t_head = ((u - flat) / taper).clamp(0., 1.);
                            let t_tail =
                                ((f64::from(wave_end - cur_x) - flat) / taper).clamp(0., 1.);
                            let env = (t_head * t_head * (3. - 2. * t_head))
                                * (t_tail * t_tail * (3. - 2. * t_tail));

                            let elevation = (baseline
                                + weights[0] * (u * w1 - p1).sin()
                                + weights[1] * (u * w2 - p2).sin()
                                + weights[2] * (u * w3 - p3).cos())
                            .max(0.);
                            let h = (amplitude as f64 * elevation * env) as f32;

                            sink.AddLine(Vector2 {
                                X: x + cur_x,
                                Y: rail_y - h,
                            });
                        }
                        sink.AddLine(Vector2 {
                            X: x + wave_end,
                            Y: rail_y + rail_h,
                        });
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        if sink.Close().is_ok()
                            && let Ok(brush) = self.target.CreateSolidColorBrush(
                                &wave_colors.map_or(
                                    D2D1_COLOR_F {
                                        r: 0.84,
                                        g: 0.88,
                                        b: 0.92,
                                        a: if back { 0.45 } else { 0.82 },
                                    },
                                    |colors| color(colors[layer]),
                                ),
                                None,
                            )
                        {
                            self.target.FillGeometry(&path, &brush, None);
                        }
                    }
                }
            }

            // 3. Solid played rail
            let bar_w = if tint.is_some() {
                played_span
            } else {
                played_span.max(if moving { 6. } else { 0. })
            };
            if rail_h > 0. && bar_w > 0. {
                self.rect(
                    x + inset,
                    rail_y,
                    bar_w,
                    rail_h,
                    rail_h / 2.,
                    wave_colors.map_or(0xd0d7de, |colors| colors[1]),
                );
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn water_bottle(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fraction: f32,
        seconds: f64,
        moving: bool,
    ) {
        unsafe {
            let cap_w = (width * 0.40).clamp(28., 36.);
            let cap_h = 9.0;
            let neck_w = (width * 0.28).clamp(20., 26.);
            let neck_h = 7.0;
            let body_x = x;
            let body_y = y + cap_h + neck_h;
            let body_w = width;
            let body_h = height - (cap_h + neck_h);
            let top_r = 14.0f32;
            let bot_r = 6.0f32;
            let k_top = 0.552_284_8 * top_r;
            let k_bot = 0.552_284_8 * bot_r;

            let cap_x = x + (width - cap_w) * 0.5;
            let cap_y = y;
            let neck_x = x + (width - neck_w) * 0.5;
            let neck_y = y + cap_h;

            // 1. Cap & knurled grooves
            self.rect(cap_x, cap_y, cap_w, cap_h, 3.0, 0x222e38);
            self.outline(cap_x, cap_y, cap_w, cap_h, 3.0, 0x3d4f5e);
            self.rect(cap_x + 3., cap_y + 1., cap_w - 6., 1.0, 0.5, 0x48ffffff);
            self.rect(
                cap_x + cap_w * 0.3 - 0.5,
                cap_y + 3.,
                1.0,
                4.0,
                0.5,
                0x364654,
            );
            self.rect(
                cap_x + cap_w * 0.5 - 0.5,
                cap_y + 3.,
                1.0,
                4.0,
                0.5,
                0x364654,
            );
            self.rect(
                cap_x + cap_w * 0.7 - 0.5,
                cap_y + 3.,
                1.0,
                4.0,
                0.5,
                0x364654,
            );

            // 2. Neck
            self.rect(neck_x, neck_y, neck_w, neck_h, 2.0, 0x0c1822);
            self.outline(neck_x, neck_y, neck_w, neck_h, 2.0, 0x1f3647);
            self.rect(neck_x + 2., neck_y + 1., 1.5, neck_h - 2., 0.75, 0x20ffffff);

            // 3. Flask Body dark tinted glass (curved shoulders + flat stable base)
            let mut opt_body_path = None;
            if let Ok(bp) = factory().and_then(|f| f.CreatePathGeometry())
                && let Ok(bsink) = bp.Open()
            {
                bsink.BeginFigure(
                    Vector2 {
                        X: body_x + top_r,
                        Y: body_y,
                    },
                    D2D1_FIGURE_BEGIN_FILLED,
                );
                // Top shoulder
                bsink.AddLine(Vector2 {
                    X: body_x + body_w - top_r,
                    Y: body_y,
                });
                // Top-right corner
                bsink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: body_x + body_w - top_r + k_top,
                        Y: body_y,
                    },
                    point2: Vector2 {
                        X: body_x + body_w,
                        Y: body_y + top_r - k_top,
                    },
                    point3: Vector2 {
                        X: body_x + body_w,
                        Y: body_y + top_r,
                    },
                });
                // Right wall
                bsink.AddLine(Vector2 {
                    X: body_x + body_w,
                    Y: body_y + body_h - bot_r,
                });
                // Bottom-right corner (gentle 6px curve to flat base)
                bsink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: body_x + body_w,
                        Y: body_y + body_h - bot_r + k_bot,
                    },
                    point2: Vector2 {
                        X: body_x + body_w - bot_r + k_bot,
                        Y: body_y + body_h,
                    },
                    point3: Vector2 {
                        X: body_x + body_w - bot_r,
                        Y: body_y + body_h,
                    },
                });
                // Flat stable base (72px wide!)
                bsink.AddLine(Vector2 {
                    X: body_x + bot_r,
                    Y: body_y + body_h,
                });
                // Bottom-left corner (gentle 6px curve from flat base)
                bsink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: body_x + bot_r - k_bot,
                        Y: body_y + body_h,
                    },
                    point2: Vector2 {
                        X: body_x,
                        Y: body_y + body_h - bot_r + k_bot,
                    },
                    point3: Vector2 {
                        X: body_x,
                        Y: body_y + body_h - bot_r,
                    },
                });
                // Left wall
                bsink.AddLine(Vector2 {
                    X: body_x,
                    Y: body_y + top_r,
                });
                // Top-left corner
                bsink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: Vector2 {
                        X: body_x,
                        Y: body_y + top_r - k_top,
                    },
                    point2: Vector2 {
                        X: body_x + top_r - k_top,
                        Y: body_y,
                    },
                    point3: Vector2 {
                        X: body_x + top_r,
                        Y: body_y,
                    },
                });
                bsink.EndFigure(D2D1_FIGURE_END_CLOSED);
                if bsink.Close().is_ok() {
                    if let Ok(dark_glass) = self.brush(0x09141d) {
                        self.target.FillGeometry(&bp, &dark_glass, None);
                    }
                    opt_body_path = Some(bp);
                }
            }

            // 4. Liquid interior (zero intermediate surface allocation, flat stable bottom)
            let inner_inset = 2.5;
            let inner_x = body_x + inner_inset;
            let inner_y = body_y + inner_inset;
            let inner_w = body_w - inner_inset * 2.0;
            let inner_h = body_h - inner_inset * 2.0;

            let frac = fraction.clamp(0.0, 1.0);
            if frac > 0.002 {
                let liquid_bottom = inner_y + inner_h;
                let fill_h = ((inner_h - 4.0) * frac).max(2.0);
                let surface_y = liquid_bottom - fill_h;
                let liquid_r = 4.0f32.min(fill_h * 0.5);
                let k_liq = 0.552_284_8 * liquid_r;

                let phase = if moving { seconds * 3.0 } else { 0.0 };
                let count = 28;

                // Layer 0: Back wave with curved bottom corners
                if let Ok(path) = factory().and_then(|f| f.CreatePathGeometry())
                    && let Ok(sink) = path.Open()
                {
                    sink.BeginFigure(
                        Vector2 {
                            X: inner_x,
                            Y: surface_y,
                        },
                        D2D1_FIGURE_BEGIN_FILLED,
                    );
                    for i in 0..=count {
                        let px = inner_w * i as f32 / count as f32;
                        let angle = px as f64 * 0.13 + phase * 0.85;
                        let wave_h = (angle.sin() * 2.8) as f32;
                        sink.AddLine(Vector2 {
                            X: inner_x + px,
                            Y: surface_y + wave_h,
                        });
                    }
                    if fill_h >= liquid_r {
                        sink.AddLine(Vector2 {
                            X: inner_x + inner_w,
                            Y: liquid_bottom - liquid_r,
                        });
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: inner_x + inner_w,
                                Y: liquid_bottom - liquid_r + k_liq,
                            },
                            point2: Vector2 {
                                X: inner_x + inner_w - liquid_r + k_liq,
                                Y: liquid_bottom,
                            },
                            point3: Vector2 {
                                X: inner_x + inner_w - liquid_r,
                                Y: liquid_bottom,
                            },
                        });
                        sink.AddLine(Vector2 {
                            X: inner_x + liquid_r,
                            Y: liquid_bottom,
                        });
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: inner_x + liquid_r - k_liq,
                                Y: liquid_bottom,
                            },
                            point2: Vector2 {
                                X: inner_x,
                                Y: liquid_bottom - liquid_r + k_liq,
                            },
                            point3: Vector2 {
                                X: inner_x,
                                Y: liquid_bottom - liquid_r,
                            },
                        });
                    } else {
                        sink.AddLine(Vector2 {
                            X: inner_x + inner_w,
                            Y: liquid_bottom,
                        });
                        sink.AddLine(Vector2 {
                            X: inner_x,
                            Y: liquid_bottom,
                        });
                    }
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    if sink.Close().is_ok()
                        && let Ok(wave_brush) = self.brush(0x703de8ff)
                    {
                        self.target.FillGeometry(&path, &wave_brush, None);
                    }
                }

                // Layer 1: Front wave + deep liquid base with flat bottom
                if let Ok(path) = factory().and_then(|f| f.CreatePathGeometry())
                    && let Ok(sink) = path.Open()
                {
                    sink.BeginFigure(
                        Vector2 {
                            X: inner_x,
                            Y: surface_y,
                        },
                        D2D1_FIGURE_BEGIN_FILLED,
                    );
                    let mut crest_points = [Vector2::default(); 29];
                    for i in 0..=count {
                        let px = inner_w * i as f32 / count as f32;
                        let angle = px as f64 * 0.17 - phase * 1.15 + 1.6;
                        let wave_h = (angle.sin() * 2.4) as f32;
                        let pt = Vector2 {
                            X: inner_x + px,
                            Y: surface_y + wave_h,
                        };
                        sink.AddLine(pt);
                        if i < crest_points.len() {
                            crest_points[i] = pt;
                        }
                    }
                    if fill_h >= liquid_r {
                        sink.AddLine(Vector2 {
                            X: inner_x + inner_w,
                            Y: liquid_bottom - liquid_r,
                        });
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: inner_x + inner_w,
                                Y: liquid_bottom - liquid_r + k_liq,
                            },
                            point2: Vector2 {
                                X: inner_x + inner_w - liquid_r + k_liq,
                                Y: liquid_bottom,
                            },
                            point3: Vector2 {
                                X: inner_x + inner_w - liquid_r,
                                Y: liquid_bottom,
                            },
                        });
                        sink.AddLine(Vector2 {
                            X: inner_x + liquid_r,
                            Y: liquid_bottom,
                        });
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: inner_x + liquid_r - k_liq,
                                Y: liquid_bottom,
                            },
                            point2: Vector2 {
                                X: inner_x,
                                Y: liquid_bottom - liquid_r + k_liq,
                            },
                            point3: Vector2 {
                                X: inner_x,
                                Y: liquid_bottom - liquid_r,
                            },
                        });
                    } else {
                        sink.AddLine(Vector2 {
                            X: inner_x + inner_w,
                            Y: liquid_bottom,
                        });
                        sink.AddLine(Vector2 {
                            X: inner_x,
                            Y: liquid_bottom,
                        });
                    }
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    if sink.Close().is_ok()
                        && let Ok(wave_brush) = self.brush(0xe83de8ff)
                    {
                        self.target.FillGeometry(&path, &wave_brush, None);
                    }

                    // Surface crest stroke
                    if let Ok(crest_path) = factory().and_then(|f| f.CreatePathGeometry())
                        && let Ok(crest_sink) = crest_path.Open()
                    {
                        crest_sink.BeginFigure(crest_points[0], D2D1_FIGURE_BEGIN_HOLLOW);
                        for &pt in crest_points.iter().skip(1) {
                            crest_sink.AddLine(pt);
                        }
                        crest_sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        if crest_sink.Close().is_ok()
                            && let Ok(crest_brush) = self.brush(0xffffffff)
                        {
                            self.target
                                .DrawGeometry(&crest_path, &crest_brush, 2.0, None);
                        }
                    }
                }

                // Micro-bubbles rising
                if fill_h > 14.0 && moving {
                    let bubbles = [
                        (0.22f32, 18.0f64, 0.0f64, 1.8f32),
                        (0.50f32, 26.0f64, 2.3f64, 1.4f32),
                        (0.74f32, 21.0f64, 4.2f64, 2.2f32),
                        (0.38f32, 23.0f64, 1.2f64, 1.5f32),
                    ];
                    for &(x_ratio, speed, seed, radius) in &bubbles {
                        let travel = (fill_h - 8.0).max(4.0) as f64;
                        let progress = ((seconds * speed + seed * 20.0) % travel) as f32;
                        let by = liquid_bottom - 4.0 - progress;
                        let wobble = ((seconds * 2.4 + seed).sin() * 2.2) as f32;
                        let bx = inner_x + 6.0 + (inner_w - 12.0) * x_ratio + wobble;
                        if by > surface_y + 4.0 && by < liquid_bottom - 2.0 {
                            self.ellipse(
                                bx - radius,
                                by - radius,
                                radius * 2.0,
                                radius * 2.0,
                                0xc03de8ff,
                            );
                            self.ellipse(
                                bx - radius * 0.4,
                                by - radius * 0.4,
                                0.8,
                                0.8,
                                0xffffffff,
                            );
                        }
                    }
                }
            }

            // 5. Specular cylindrical glass reflections
            self.rect(
                body_x + 6.0,
                body_y + top_r,
                3.0,
                body_h - top_r - bot_r - 6.0,
                1.5,
                0x22ffffff,
            );
            self.rect(
                body_x + 11.0,
                body_y + top_r + 6.0,
                1.2,
                body_h - top_r - bot_r - 18.0,
                0.6,
                0x10ffffff,
            );
            self.rect(
                body_x + body_w - 7.0,
                body_y + top_r + 4.0,
                1.5,
                body_h - top_r - bot_r - 12.0,
                0.75,
                0x12ffffff,
            );
            self.rect(
                body_x + top_r,
                body_y + 1.5,
                body_w - top_r * 2.0,
                1.0,
                0.5,
                0x25ffffff,
            );

            // 6. Base boot reinforcement (stable tumbler base)
            let boot_h = 7.0f32;
            let boot_y = body_y + body_h - boot_h;
            self.rect(body_x + 1.0, boot_y, body_w - 2.0, boot_h, 4.0, 0x14202b);
            self.outline(body_x + 1.0, boot_y, body_w - 2.0, boot_h, 4.0, 0x1e3242);
            self.rect(
                body_x + 8.0,
                body_y + body_h - 2.0,
                body_w - 16.0,
                1.0,
                0.5,
                0x15ffffff,
            );

            // 7. Outer bottle glass outline
            if let Some(bp) = opt_body_path
                && let Ok(outline_brush) = self.brush(0x203c4f)
            {
                self.target.DrawGeometry(&bp, &outline_brush, 1.0, None);
            }
            self.rect(
                body_x + top_r,
                body_y,
                body_w - top_r * 2.0,
                1.0,
                0.5,
                0x403de8ff,
            );
        }
    }
    pub(super) unsafe fn finish_shell_clip(&self) {
        if self.shell_layer.replace(false) {
            unsafe { self.target.PopLayer() };
        }
    }
    pub unsafe fn end(&self) -> Result<()> {
        unsafe {
            self.finish_shell_clip();
            let result = self.target.EndDraw(None, None);
            self.presented.set(result.is_ok());
            result
        }
    }
    pub unsafe fn rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: u32) {
        unsafe {
            if let Ok(b) = self.brush(c) {
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: r,
                        radiusY: r,
                    },
                    &b,
                );
            }
        }
    }
    pub unsafe fn outline(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: u32) {
        unsafe {
            if let Ok(b) = self.brush(c) {
                self.target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: r,
                        radiusY: r,
                    },
                    &b,
                    1.,
                    None,
                );
            }
        }
    }
    pub unsafe fn line(&self, a: (f32, f32), b: (f32, f32), weight: f32, c: u32) {
        unsafe {
            if let Ok(brush) = self.brush(c) {
                self.target.DrawLine(
                    Vector2 { X: a.0, Y: a.1 },
                    Vector2 { X: b.0, Y: b.1 },
                    &brush,
                    weight,
                    Some(&self.stroke),
                );
            }
        }
    }
    pub(super) unsafe fn polyline(&self, points: &[(f32, f32)], weight: f32, c: u32) -> Result<()> {
        if let Some((&(x, y), rest)) = points.split_first()
            && !rest.is_empty()
        {
            unsafe {
                let path = factory()?.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(Vector2 { X: x, Y: y }, D2D1_FIGURE_BEGIN_HOLLOW);
                for &(x, y) in rest {
                    sink.AddLine(Vector2 { X: x, Y: y });
                }
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                self.target
                    .DrawGeometry(&path, &self.brush(c)?, weight, Some(&self.stroke));
            }
        }
        Ok(())
    }
    pub unsafe fn ellipse(&self, x: f32, y: f32, w: f32, h: f32, c: u32) {
        unsafe {
            if let Ok(brush) = self.brush(c) {
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 {
                            X: x + w / 2.,
                            Y: y + h / 2.,
                        },
                        radiusX: w / 2.,
                        radiusY: h / 2.,
                    },
                    &brush,
                );
            }
        }
    }
    pub unsafe fn calendar_marker(&self, x: f32, y: f32, selected: bool, today: bool, hover: f32) {
        unsafe {
            if !selected && !today && hover < 0.001 {
                return;
            }
            let shape = D2D1_ELLIPSE {
                point: Vector2 {
                    X: x + 11.,
                    Y: y + 11.,
                },
                radiusX: 10.5,
                radiusY: 10.5,
            };
            if selected {
                if let Ok(stops) = self.target.CreateGradientStopCollection(
                    &[
                        D2D1_GRADIENT_STOP {
                            position: 0.,
                            color: color(0x76548f),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.,
                            color: color(0x432850),
                        },
                    ],
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 { X: x, Y: y },
                        endPoint: Vector2 { X: x, Y: y + 22. },
                    },
                    None,
                    &stops,
                ) {
                    self.target.FillEllipse(&shape, &brush);
                }
            } else if hover > 0.001 {
                self.ellipse(
                    x + 0.5,
                    y + 0.5,
                    21.,
                    21.,
                    crate::motion::mix(0x191919, 0x3d3048, hover),
                );
            }
            if today && !selected {
                if let Ok(brush) = self.brush(0xd99bff) {
                    self.target.DrawEllipse(&shape, &brush, 1.2, None);
                }
            } else if let Ok(stops) = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(if selected { 0xb0ffffff } else { 0x40ffffff }),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(if selected { 0x35ffffff } else { 0x10ffffff }),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: x, Y: y },
                    endPoint: Vector2 { X: x, Y: y + 22. },
                },
                None,
                &stops,
            ) {
                self.target
                    .DrawEllipse(&shape, &brush, if today { 1.2 } else { 0.8 }, None);
            }
        }
    }
    pub unsafe fn polygon(&self, points: &[(f32, f32)], c: u32) {
        unsafe {
            if points.len() < 3 {
                return;
            }
            if let Ok(path) = factory().and_then(|f| f.CreatePathGeometry())
                && let Ok(sink) = path.Open()
            {
                sink.BeginFigure(
                    Vector2 {
                        X: points[0].0,
                        Y: points[0].1,
                    },
                    D2D1_FIGURE_BEGIN_FILLED,
                );
                for &(x, y) in &points[1..] {
                    sink.AddLine(Vector2 { X: x, Y: y });
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                if sink.Close().is_ok()
                    && let Ok(brush) = self.brush(c)
                {
                    self.target.FillGeometry(&path, &brush, None);
                }
            }
        }
    }
    pub unsafe fn presence_eyes(&self, b: (f32, f32, f32, f32), pose: crate::mascot::EyePose) {
        unsafe {
            let slit = EYE_SLIT.with(|cell| -> Result<ID2D1Geometry> {
                if let Some(geometry) = cell.get() {
                    return Ok(geometry.clone());
                }
                // One rounded outer edge and one cut inner edge, mirrored across the pair.
                let geometry = crate::icons::from_svg(
                    &factory()?,
                    include_str!("../assets/branding/eye.svg"),
                )?;
                let _ = cell.set(geometry.clone());
                Ok(geometry)
            });
            let Ok(slit) = slit else { return };
            if self.eye_glass.get().is_none()
                && let Ok(stops) = self.target.CreateGradientStopCollection(
                    &[
                        (0., 0xf0ffffff),
                        (0.32, 0xdcd3dce7),
                        (0.82, 0xc1768293),
                        (1., 0xd2cad4e0),
                    ]
                    .map(|(position, ink)| D2D1_GRADIENT_STOP {
                        position,
                        color: color(ink),
                    }),
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )
                && let Ok(brush) = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 { X: 6., Y: 0. },
                        endPoint: Vector2 { X: 8., Y: 8. },
                    },
                    None,
                    &stops,
                )
            {
                let _ = self.eye_glass.set(brush);
            }
            let scale = (b.2 / 64.).clamp(0.6, 1.28);
            let cx = b.0 + b.2 / 2. + pose.look.0.clamp(-1., 1.) * 3.2 * scale;
            let cy = b.1 + b.3 / 2. + pose.look.1.clamp(-1., 1.) * 2. * scale;
            let mut previous = Matrix3x2::default();
            self.target.GetTransform(&mut previous);
            for (index, openness) in pose.open.into_iter().enumerate() {
                let open = openness.clamp(0., 1.);
                let x = cx + (if index == 0 { -18.5 } else { 2.5 }) * scale;
                let height = (1.25 + open * 6.25) * scale;
                let y = cy - height / 2.;
                let glow = (open * 11.).round() as u32;
                if glow > 0 {
                    self.ellipse(
                        x - 2. * scale,
                        y - 2. * scale,
                        20. * scale,
                        height + 4. * scale,
                        (glow << 24) | 0xd7e2ef,
                    );
                }
                let (sx, origin) = if index == 0 {
                    (scale, x)
                } else {
                    (-scale, x + 16. * scale)
                };
                let sy = height / 8.;
                let transform = Matrix3x2 {
                    M11: sx * previous.M11,
                    M12: sx * previous.M12,
                    M21: sy * previous.M21,
                    M22: sy * previous.M22,
                    M31: origin * previous.M11 + y * previous.M21 + previous.M31,
                    M32: origin * previous.M12 + y * previous.M22 + previous.M32,
                };
                self.target.SetTransform(&transform);
                if let Some(brush) = self.eye_glass.get() {
                    brush.SetOpacity(0.6 + open * 0.4);
                    self.target.FillGeometry(&slit, brush, None);
                } else if let Ok(brush) = self.brush(crate::motion::mix(0x727a85, 0xdbe2eb, open)) {
                    self.target.FillGeometry(&slit, &brush, None);
                }
                self.target.SetTransform(&previous);
                let highlight = ((open - 0.4).max(0.) * 280.).round() as u32;
                if highlight > 0 {
                    self.line(
                        (x + 4. * scale, y + 1.35 * scale),
                        (x + 11. * scale, y + 1.35 * scale),
                        0.65 * scale,
                        (highlight << 24) | 0xffffff,
                    );
                }
            }
            if pose.sleep > 0. {
                for offset in [0., 1. / 3., 2. / 3.] {
                    let travel = (pose.sleep_phase + offset).fract();
                    let fade = (travel / 0.16).min((1. - travel) / 0.28).min(1.);
                    let alpha = (240. * pose.sleep * fade).round() as u32;
                    if alpha == 0 {
                        continue;
                    }
                    let x = cx + (6. + 17. * travel) * scale;
                    let y = cy - (7. + 10. * travel) * scale;
                    let end = (3. + 2.6 * travel) * scale;
                    let stroke = 1.05 * scale;
                    let opacity = alpha << 24;
                    self.line((x, y), (x + end, y), stroke, opacity | 0xf6faff);
                    self.line((x + end, y), (x, y + end), stroke, opacity | 0xc6d0de);
                    self.line((x, y + end), (x + end, y + end), stroke, opacity | 0x98a6b8);
                }
            }
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing text includes its bounds and typography"
    )]
    pub unsafe fn text(&self, text: &str, x: f32, y: f32, w: f32, size: f32, bold: bool, c: u32) {
        unsafe { self.label(text, x, y, w, size, bold, c, 0) }
    }
    pub unsafe fn design_asset(
        &self,
        name: &'static str,
        bytes: &[u8],
        b: (f32, f32, f32, f32),
    ) -> Result<()> {
        unsafe {
            let mut assets = self.design_assets.borrow_mut();
            if !assets.contains_key(name) {
                let source = crate::artwork::source_native(bytes)?;
                assets.insert(name, self.target.CreateBitmapFromWicBitmap(&source, None)?);
            }
            let mut previous = Matrix3x2::default();
            self.target.GetTransform(&mut previous);
            if matches!(name, "calendar-next" | "media-next") {
                self.target.SetTransform(&Matrix3x2 {
                    M11: -previous.M11,
                    M22: if name == "media-next" {
                        previous.M22
                    } else {
                        -previous.M22
                    },
                    M31: previous.M31 + (2. * b.0 + b.2) * previous.M11,
                    M32: if name == "media-next" {
                        previous.M32
                    } else {
                        previous.M32 + 2. * b.1 + b.3
                    },
                    ..Default::default()
                });
            }
            if name == "fire" {
                let brush = self.brush(0xffffff)?;
                let antialias = self.target.GetAntialiasMode();
                self.target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
                self.target.FillOpacityMask(
                    &assets[name],
                    &brush,
                    D2D1_OPACITY_MASK_CONTENT_GRAPHICS,
                    Some(&rect(b.0, b.1, b.2, b.3)),
                    None,
                );
                self.target.SetAntialiasMode(antialias);
            } else {
                self.target.DrawBitmap(
                    &assets[name],
                    Some(&rect(b.0, b.1, b.2, b.3)),
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
            if matches!(name, "calendar-next" | "media-next") {
                self.target.SetTransform(&previous);
            }
            Ok(())
        }
    }
    pub unsafe fn design_scale(&self, scale: f32) {
        unsafe {
            self.design_transform(scale, 0.);
        }
    }
    pub unsafe fn design_transform(&self, scale: f32, offset: f32) {
        unsafe { self.design_transform_y(scale, offset, 0.) }
    }
    pub unsafe fn design_transform_y(&self, scale: f32, offset: f32, y: f32) {
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: scale,
                M22: 1.,
                M31: offset,
                M32: y,
                ..Default::default()
            });
        }
    }
    unsafe fn design_format(&self, style: (f32, u16, u8)) -> Result<IDWriteTextFormat> {
        unsafe {
            let (size, weight, align) = style;
            let key = ((size * 10.) as u16, weight, align);
            let mut formats = self.design_formats.borrow_mut();
            if let Some(format) = formats.get(&key) {
                return Ok(format.clone());
            }
            let f = self.write.CreateTextFormat(
                w!("Poppins"),
                &self.fonts,
                DWRITE_FONT_WEIGHT(weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("id-ID"),
            )?;
            f.SetWordWrapping(if matches!(align, 2 | 3) {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            })?;
            f.SetTextAlignment(match align {
                1 | 2 | DESIGN_CENTERED => DWRITE_TEXT_ALIGNMENT_CENTER,
                4 => DWRITE_TEXT_ALIGNMENT_TRAILING,
                _ => DWRITE_TEXT_ALIGNMENT_LEADING,
            })?;
            if align == DESIGN_CENTERED {
                f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            }
            f.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, size * 1.5, size * 1.1)?;
            if let Ok(sign) = self.write.CreateEllipsisTrimmingSign(&f) {
                let _ = f.SetTrimming(
                    &DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        ..Default::default()
                    },
                    &sign,
                );
            }
            formats.insert(key, f.clone());
            Ok(f)
        }
    }
    pub unsafe fn design_measure(
        &self,
        text: &str,
        width: f32,
        style: (f32, u16, u8),
    ) -> (f32, f32) {
        unsafe {
            let wk = (width * 10.).round() as u16;
            let sk = (style.0 * 10.).round() as u16;
            if let Some(value) =
                self.design_measurements.borrow().iter().find(|v| {
                    v.0 == text && v.1 == wk && v.2 == sk && v.3 == style.1 && v.4 == style.2
                })
            {
                return (value.5, value.6);
            }
            let measured = (|| -> Result<(f32, f32)> {
                let format = self.design_format(style)?;
                let chars: Vec<u16> = text.encode_utf16().collect();
                let layout = self
                    .write
                    .CreateTextLayout(&chars, &format, width.max(1.), 10000.)?;
                let mut metrics = DWRITE_TEXT_METRICS::default();
                layout.GetMetrics(&mut metrics)?;
                Ok((
                    metrics.widthIncludingTrailingWhitespace,
                    metrics.height.max(style.0 * 1.5),
                ))
            })()
            .unwrap_or((width, style.0 * 1.5));
            if text.len() <= 4096 {
                let mut cache = self.design_measurements.borrow_mut();
                if cache.len() >= 128 {
                    cache.pop_front();
                }
                cache.push_back((
                    text.into(),
                    wk,
                    sk,
                    style.1,
                    style.2,
                    measured.0,
                    measured.1,
                ));
            }
            measured
        }
    }
    pub unsafe fn push_design_clip(&self, b: (f32, f32, f32, f32)) {
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect(b.0, b.1, b.2, b.3), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }
    pub unsafe fn pop_design_clip(&self) {
        unsafe {
            self.target.PopAxisAlignedClip();
        }
    }
    pub unsafe fn push_design_opacity(&self, b: (f32, f32, f32, f32), opacity: f32) {
        let parameters = D2D1_LAYER_PARAMETERS {
            contentBounds: rect(b.0, b.1, b.2, b.3),
            maskTransform: Matrix3x2 {
                M11: 1.,
                M22: 1.,
                ..Default::default()
            },
            opacity,
            ..Default::default()
        };
        unsafe { self.target.PushLayer(&parameters, None) };
    }
    pub unsafe fn pop_design_opacity(&self) {
        unsafe { self.target.PopLayer() };
    }
    pub unsafe fn design_karaoke(
        &self,
        line: &crate::lyrics::Line,
        timing: (f64, f64),
        b: (f32, f32, f32, f32),
        style: (f32, u16, u8),
        scroll: bool,
        opacity: f32,
    ) {
        let alpha = (opacity.clamp(0., 1.) * 255.).round() as u32;
        if alpha == 0 || line.text.is_empty() {
            return;
        }
        let tint = |rgb: u32| (alpha << 24) | rgb;
        unsafe {
            let width_key = (b.2 * 10.).round() as u32;
            let style_key = ((style.0 * 10.) as u16, style.1, style.2);
            let layout = (|| -> Result<IDWriteTextLayout> {
                let mut cache = self.karaoke_layout.borrow_mut();
                if let Some(index) = cache.iter().position(|old| {
                    old.text == line.text
                        && old.width == width_key
                        && old.style == style_key
                        && old.scroll == scroll
                }) {
                    let old = cache.remove(index).unwrap();
                    let layout = old.layout.clone();
                    cache.push_front(old);
                    return Ok(layout);
                }
                let format =
                    self.design_format((style.0, style.1, if scroll { 0 } else { style.2 }))?;
                let layout = self.write.CreateTextLayout(
                    &line.text.encode_utf16().collect::<Vec<_>>(),
                    &format,
                    if scroll { 10000. } else { b.2.max(1.) },
                    10000.,
                )?;
                if cache.len() == 2 {
                    cache.pop_back();
                }
                cache.push_front(KaraokeLayout {
                    text: line.text.clone(),
                    width: width_key,
                    style: style_key,
                    scroll,
                    layout: layout.clone(),
                });
                Ok(layout)
            })();
            let Ok(layout) = layout else {
                self.design_text(&line.text, b, style, tint(0xffffff));
                return;
            };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return;
            }
            let count = line.text.encode_utf16().count() as u32;
            let units = line.sung_units(timing.0, timing.1).clamp(0., count as f32);
            let mut hit = DWRITE_HIT_TEST_METRICS::default();
            let (mut caret_x, mut caret_y) = (0., 0.);
            if layout
                .HitTestTextPosition(
                    (units.floor() as u32).min(count.saturating_sub(1)),
                    false,
                    &mut caret_x,
                    &mut caret_y,
                    &mut hit,
                )
                .is_err()
            {
                return;
            }
            let fraction =
                ((units - hit.textPosition as f32) / hit.length.max(1) as f32).clamp(0., 1.);
            let caret = hit.left + hit.width * fraction;
            let offset = if scroll {
                crate::layout::karaoke_scroll(caret, metrics.widthIncludingTrailingWhitespace, b.2)
            } else {
                0.
            };
            let center = if scroll && matches!(style.2, 1 | 2) {
                ((b.2 - metrics.widthIncludingTrailingWhitespace) / 2.).max(0.)
            } else {
                0.
            };
            let origin = windows_numerics::Vector2 {
                X: b.0 + center - offset,
                Y: b.1,
            };
            self.push_design_clip(b);
            if let Ok(brush) = self.brush(tint(0x828b98)) {
                self.target
                    .DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
            if units > 0.
                && let Ok(brush) = self.brush(tint(0xffffff))
            {
                let mut regions = Vec::new();
                if hit.textPosition > 0 {
                    let mut needed = 0;
                    let _ = layout.HitTestTextRange(0, hit.textPosition, 0., 0., None, &mut needed);
                    if needed <= 256 {
                        regions.resize(needed as usize, DWRITE_HIT_TEST_METRICS::default());
                        if layout
                            .HitTestTextRange(
                                0,
                                hit.textPosition,
                                0.,
                                0.,
                                Some(&mut regions),
                                &mut needed,
                            )
                            .is_err()
                        {
                            regions.clear();
                        }
                    }
                }
                if fraction > 0. {
                    hit.width *= fraction;
                    regions.push(hit);
                }
                for region in regions {
                    self.push_design_clip((
                        origin.X + region.left,
                        origin.Y + region.top,
                        region.width,
                        region.height,
                    ));
                    self.target.DrawTextLayout(
                        origin,
                        &layout,
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                    self.pop_design_clip();
                }
            }
            self.pop_design_clip();
        }
    }
    pub unsafe fn design_text(
        &self,
        text: &str,
        b: (f32, f32, f32, f32),
        style: (f32, u16, u8),
        ink: u32,
    ) {
        unsafe {
            if let (Ok(format), Ok(brush)) = (self.design_format(style), self.brush(ink)) {
                self.target.DrawText(
                    &text.encode_utf16().collect::<Vec<_>>(),
                    &format,
                    &rect(b.0, b.1, b.2, b.3),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }
    pub unsafe fn design_glass_clock(&self, text: &str, bounds: (f32, f32, f32, f32)) {
        unsafe {
            self.design_glass_clock_sized(text, bounds, 36.);
        }
    }
    pub unsafe fn design_glass_clock_sized(
        &self,
        text: &str,
        bounds: (f32, f32, f32, f32),
        size: f32,
    ) {
        unsafe {
            let Ok(format) = self.design_format((size, 600, 0)) else {
                return;
            };
            let text = text.encode_utf16().collect::<Vec<_>>();
            let (x, y, w, h) = bounds;
            // Subpixel rim and shallow lower bevel belong to the glyphs only;
            // the clock has no glow or translucent rectangular backdrop.
            for (dx, dy, ink) in [(-0.45, -0.45, 0x58ffffff), (0.55, 0.7, 0x80363840)] {
                if let Ok(brush) = self.brush(ink) {
                    self.target.DrawText(
                        &text,
                        &format,
                        &rect(x + dx, y + dy, w, h),
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
            let stops = self.target.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP {
                        position: 0.,
                        color: color(0xf4f7fb),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.4,
                        color: color(0xa8b0bb),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.48,
                        color: color(0xe3e8ee),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 0.57,
                        color: color(0x919ba8),
                    },
                    D2D1_GRADIENT_STOP {
                        position: 1.,
                        color: color(0x737d8b),
                    },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            );
            if let Ok(stops) = stops
                && let Ok(brush) = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 { X: x, Y: y + 12. },
                        endPoint: Vector2 {
                            X: x + 18.,
                            Y: y + 43.,
                        },
                    },
                    None,
                    &stops,
                )
            {
                self.target.DrawText(
                    &text,
                    &format,
                    &rect(x, y, w, h),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }
    pub unsafe fn liquid_level(
        &self,
        b: (f32, f32, f32, f32),
        fraction: f32,
        seconds: f32,
        ink: u32,
    ) {
        unsafe {
            let (x, y, w, h) = b;
            let mask: ID2D1Geometry = match factory()
                .and_then(|f| {
                    f.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                        rect: rect(x, y, w, h),
                        radiusX: if h < 100. { 11. } else { 30. },
                        radiusY: if h < 100. { 11. } else { 30. },
                    })
                })
                .and_then(|m| m.cast())
            {
                Ok(m) => m,
                Err(_) => return,
            };
            let mut parameters = D2D1_LAYER_PARAMETERS {
                contentBounds: rect(x, y, w, h),
                geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2 {
                    M11: 1.,
                    M22: 1.,
                    ..Default::default()
                },
                opacity: 1.,
                ..Default::default()
            };
            self.target.PushLayer(&parameters, None);
            let level = y + h * (1. - fraction.clamp(0., 1.));
            let mut points = Vec::with_capacity(35);
            points.push((x, y + h));
            for i in 0..=32 {
                let xx = i as f32 / 32.;
                points.push((
                    x + w * xx,
                    level
                        + ((xx * 18. + seconds * 2.).sin() + (xx * 47. - seconds * 1.4).sin())
                            * 1.1,
                ));
            }
            points.push((x + w, y + h));
            self.polygon(&points, ink);
            self.target.PopLayer();
            std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing text includes its bounds and typography"
    )]
    pub unsafe fn center(&self, text: &str, x: f32, y: f32, w: f32, size: f32, bold: bool, c: u32) {
        unsafe { self.label(text, x, y, w, size, bold, c, 1) }
    }
    pub unsafe fn measure_text(&self, text: &str, size: f32, bold: bool) -> f32 {
        unsafe {
            let key = ((size * 10.) as u16, bold, 2);
            let mut formats = self.formats.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = formats.entry(key)
                && let Ok(f) = self.write.CreateTextFormat(
                    w!("Plus Jakarta Sans"),
                    &self.fonts,
                    if bold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_MEDIUM
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("id-ID"),
                )
            {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                entry.insert(f);
            }
            if let Some(format) = formats.get(&key) {
                let chars: Vec<u16> = text.encode_utf16().collect();
                if let Ok(layout) = self
                    .write
                    .CreateTextLayout(&chars, format, 10000., size * 2.)
                {
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    if layout.GetMetrics(&mut metrics).is_ok() {
                        return metrics.width;
                    }
                }
            }
            text.chars().count() as f32 * size * 0.6
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing marquee text includes bounds, typography and animation phase"
    )]
    pub unsafe fn marquee(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        size: f32,
        bold: bool,
        c: u32,
        elapsed_sec: f64,
    ) {
        unsafe {
            let text_w = self.measure_text(text, size, bold);
            if text_w <= w {
                self.center(text, x, y, w, size, bold, c);
                return;
            }
            let overflow = text_w - w;
            let pause_start = 1.8;
            let pause_end = 1.4;
            let speed = 36.0;
            let scroll_duration = (overflow as f64 + 18.0) / speed;
            let reset_duration = 0.8;
            let total_cycle = pause_start + scroll_duration + pause_end + reset_duration;
            let phase = elapsed_sec % total_cycle;

            let offset = if phase < pause_start {
                0.0
            } else if phase < pause_start + scroll_duration {
                let progress = ((phase - pause_start) / scroll_duration) as f32;
                progress * (overflow + 18.0)
            } else if phase < pause_start + scroll_duration + pause_end {
                overflow + 18.0
            } else {
                let r =
                    ((phase - (pause_start + scroll_duration + pause_end)) / reset_duration) as f32;
                (overflow + 18.0) * (1.0 - crate::motion::ease(r))
            };

            self.target.PushAxisAlignedClip(
                &rect(x, y - 2., w, size * 2. + 4.),
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );

            let key = ((size * 10.) as u16, bold, 2);
            let mut formats = self.formats.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = formats.entry(key)
                && let Ok(f) = self.write.CreateTextFormat(
                    w!("Plus Jakarta Sans"),
                    &self.fonts,
                    if bold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_MEDIUM
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("id-ID"),
                )
            {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                entry.insert(f);
            }
            if let (Some(format), Ok(brush)) = (formats.get(&key), self.brush(c)) {
                let chars: Vec<u16> = text.encode_utf16().collect();
                self.target.DrawText(
                    &chars,
                    format,
                    &rect(x - offset, y, text_w + 32., size * 1.8),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            self.target.PopAxisAlignedClip();
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing marquee text includes bounds, typography and animation phase"
    )]
    pub unsafe fn marquee_left(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        size: f32,
        bold: bool,
        c: u32,
        elapsed_sec: f64,
    ) {
        unsafe {
            let text_w = self.measure_text(text, size, bold);
            if text_w <= w {
                self.text(text, x, y, w, size, bold, c);
                return;
            }
            let overflow = text_w - w;
            let pause_start = 1.8;
            let pause_end = 1.4;
            let speed = 36.0;
            let scroll_duration = (overflow as f64 + 18.0) / speed;
            let reset_duration = 0.8;
            let total_cycle = pause_start + scroll_duration + pause_end + reset_duration;
            let phase = elapsed_sec % total_cycle;

            let offset = if phase < pause_start {
                0.0
            } else if phase < pause_start + scroll_duration {
                let progress = ((phase - pause_start) / scroll_duration) as f32;
                progress * (overflow + 18.0)
            } else if phase < pause_start + scroll_duration + pause_end {
                overflow + 18.0
            } else {
                let r =
                    ((phase - (pause_start + scroll_duration + pause_end)) / reset_duration) as f32;
                (overflow + 18.0) * (1.0 - crate::motion::ease(r))
            };

            self.target.PushAxisAlignedClip(
                &rect(x, y - 2., w, size * 2. + 4.),
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );

            let key = ((size * 10.) as u16, bold, 2);
            let mut formats = self.formats.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = formats.entry(key)
                && let Ok(f) = self.write.CreateTextFormat(
                    w!("Plus Jakarta Sans"),
                    &self.fonts,
                    if bold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_MEDIUM
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("id-ID"),
                )
            {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                entry.insert(f);
            }
            if let (Some(format), Ok(brush)) = (formats.get(&key), self.brush(c)) {
                let chars: Vec<u16> = text.encode_utf16().collect();
                self.target.DrawText(
                    &chars,
                    format,
                    &rect(x - offset, y, text_w + 32., size * 1.8),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            self.target.PopAxisAlignedClip();
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing lyric scroll text includes bounds, typography and progress"
    )]
    pub unsafe fn lyric_scroll(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        size: f32,
        bold: bool,
        c: u32,
        line_progress: f32,
    ) {
        unsafe {
            let text_w = self.measure_text(text, size, bold);
            let progress = line_progress.clamp(0.0, 1.0);
            let active_c = c;
            let inactive_c = 0x525c68;

            let key = ((size * 10.) as u16, bold, 2);
            let mut formats = self.formats.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = formats.entry(key)
                && let Ok(f) = self.write.CreateTextFormat(
                    w!("Plus Jakarta Sans"),
                    &self.fonts,
                    if bold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_MEDIUM
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("id-ID"),
                )
            {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                entry.insert(f);
            }

            let Some(format) = formats.get(&key) else {
                return;
            };
            let chars: Vec<u16> = text.encode_utf16().collect();

            if text_w <= w {
                // 1. Draw entire line in dimmed unplayed color
                if let Ok(brush) = self.brush(inactive_c) {
                    self.target.DrawText(
                        &chars,
                        format,
                        &rect(x, y, text_w + 32., size * 1.8),
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
                // 2. Overlay sung portion in crisp active color with clipping
                if progress > 0.001
                    && let Ok(brush) = self.brush(active_c)
                {
                    let active_w = text_w * progress;
                    self.target.PushAxisAlignedClip(
                        &rect(x, y - 2., active_w, size * 2. + 4.),
                        D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    );
                    self.target.DrawText(
                        &chars,
                        format,
                        &rect(x, y, text_w + 32., size * 1.8),
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    self.target.PopAxisAlignedClip();
                }
                return;
            }

            let overflow = text_w - w;
            let offset = if progress < 0.10 {
                0.0
            } else if progress < 0.90 {
                let p = (progress - 0.10) / (0.90 - 0.10);
                crate::motion::ease(p) * (overflow + 14.0)
            } else {
                overflow + 14.0
            };

            self.target.PushAxisAlignedClip(
                &rect(x, y - 2., w, size * 2. + 4.),
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );

            // 1. Draw entire scrolled text in dimmed unplayed color
            if let Ok(brush) = self.brush(inactive_c) {
                self.target.DrawText(
                    &chars,
                    format,
                    &rect(x - offset, y, text_w + 32., size * 1.8),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // 2. Overlay sung portion in crisp active color with clip
            if progress > 0.001
                && let Ok(brush) = self.brush(active_c)
            {
                let sweep_w = (text_w * progress - offset).clamp(0.0, w);
                if sweep_w > 0.0 {
                    self.target.PushAxisAlignedClip(
                        &rect(x, y - 2., sweep_w, size * 2. + 4.),
                        D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    );
                    self.target.DrawText(
                        &chars,
                        format,
                        &rect(x - offset, y, text_w + 32., size * 1.8),
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    self.target.PopAxisAlignedClip();
                }
            }

            self.target.PopAxisAlignedClip();
        }
    }
    pub unsafe fn draw_contour(
        &self,
        width: f32,
        height: f32,
        p: f32,
        radius: f32,
        offset: f32,
        c: u32,
    ) {
        unsafe {
            let pts = crate::layout::contour(width, height, p, radius);
            if pts.len() < 3 {
                return;
            }
            if let Ok(path) = factory().and_then(|f| f.CreatePathGeometry())
                && let Ok(sink) = path.Open()
            {
                // Stroke starts at pts[1] (w, 0.) and follows shoulders, sides, and bottom,
                // leaving the top screen bezel (y=0) flush without an artificial line.
                sink.BeginFigure(
                    Vector2 {
                        X: pts[1].0 + offset,
                        Y: pts[1].1,
                    },
                    D2D1_FIGURE_BEGIN_HOLLOW,
                );
                for pt in &pts[2..] {
                    sink.AddLine(Vector2 {
                        X: pt.0 + offset,
                        Y: pt.1,
                    });
                }
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                if sink.Close().is_ok()
                    && let Ok(brush) = self.brush(c)
                {
                    self.target.DrawGeometry(&path, &brush, 1.0, None);
                }
            }
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Drawing text includes its bounds and typography"
    )]
    unsafe fn label(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        size: f32,
        bold: bool,
        c: u32,
        align: u8,
    ) {
        unsafe {
            let key = ((size * 10.) as u16, bold, align);
            let mut formats = self.formats.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = formats.entry(key)
                && let Ok(f) = self.write.CreateTextFormat(
                    w!("Plus Jakarta Sans"),
                    &self.fonts,
                    if bold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_MEDIUM
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("id-ID"),
                )
            {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetTextAlignment(if align == 1 {
                    DWRITE_TEXT_ALIGNMENT_CENTER
                } else {
                    DWRITE_TEXT_ALIGNMENT_LEADING
                });
                if let Ok(sign) = self.write.CreateEllipsisTrimmingSign(&f) {
                    let _ = f.SetTrimming(
                        &DWRITE_TRIMMING {
                            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                            ..Default::default()
                        },
                        &sign,
                    );
                }
                entry.insert(f);
            }
            if let (Some(format), Ok(brush)) = (formats.get(&key), self.brush(c)) {
                let chars: Vec<u16> = text.encode_utf16().collect();
                self.target.DrawText(
                    &chars,
                    format,
                    &rect(x, y, w, size * 1.8),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }
    pub unsafe fn icon(&self, name: &'static str, x: f32, y: f32, size: f32, _c: u32) {
        unsafe { self.draw_icon(name, x, y, size, 0xffffff) }
    }
    unsafe fn draw_icon(&self, name: &'static str, x: f32, y: f32, size: f32, ink: u32) {
        unsafe {
            let result = ICONS.with(|icons| -> Result<ID2D1Geometry> {
                if let Some(g) = icons.borrow().get(name) {
                    return Ok(g.clone());
                }
                let g = crate::icons::outline(&factory()?, name)?;
                icons.borrow_mut().insert(name, g.clone());
                Ok(g)
            });
            if let (Ok(g), Ok(b)) = (result, self.brush(ink)) {
                let full_scale = size / 24.;
                let scale = full_scale * self.icon_scale.get();
                let (x, y) = if scale != full_scale
                    && let Ok(ink) = g.GetBounds(None)
                {
                    let inset = (full_scale - scale) / 2.;
                    (
                        x + (ink.left + ink.right) * inset,
                        y + (ink.top + ink.bottom) * inset,
                    )
                } else {
                    (x, y)
                };
                let mut transform = Matrix3x2 {
                    M11: scale,
                    M12: 0.,
                    M21: 0.,
                    M22: scale,
                    M31: x,
                    M32: y,
                };
                let mut previous = Matrix3x2::default();
                self.target.GetTransform(&mut previous);
                transform.M11 = scale * previous.M11;
                transform.M12 = scale * previous.M12;
                transform.M21 = scale * previous.M21;
                transform.M22 = scale * previous.M22;
                transform.M31 = x * previous.M11 + y * previous.M21 + previous.M31;
                transform.M32 = x * previous.M12 + y * previous.M22 + previous.M32;
                self.target.SetTransform(&transform);
                self.target.FillGeometry(&g, &b, None);

                self.target.SetTransform(&previous);
            }
        }
    }
    pub unsafe fn icon_centered(
        &self,
        name: &'static str,
        bounds: (f32, f32, f32, f32),
        size: f32,
    ) {
        unsafe { self.icon_centered_tinted(name, bounds, size, 0xffffff) }
    }
    pub unsafe fn icon_centered_tinted(
        &self,
        name: &'static str,
        bounds: (f32, f32, f32, f32),
        size: f32,
        tint: u32,
    ) {
        unsafe {
            let geometry = ICONS.with(|icons| -> Result<ID2D1Geometry> {
                if let Some(g) = icons.borrow().get(name) {
                    return Ok(g.clone());
                }
                let g = crate::icons::outline(&factory()?, name)?;
                icons.borrow_mut().insert(name, g.clone());
                Ok(g)
            });
            if let Ok(g) = geometry
                && let Ok(ink) = g.GetBounds(None)
            {
                let scale = size / 24.;
                self.draw_icon(
                    name,
                    bounds.0 + bounds.2 / 2. - (ink.left + ink.right) * 0.5 * scale,
                    bounds.1 + bounds.3 / 2. - (ink.top + ink.bottom) * 0.5 * scale,
                    size,
                    tint,
                );
            }
        }
    }
    pub unsafe fn icon_filled(&self, name: &'static str, x: f32, y: f32, size: f32, color: u32) {
        unsafe {
            let actual_name = match name {
                "heart" => "heart-fill",
                other => other,
            };
            self.icon(actual_name, x, y, size, color);
        }
    }
    pub unsafe fn save_bmp(&self, path: &Path) -> Result<()> {
        unsafe {
            let Some(bitmap) = &self.bitmap else {
                return Err(Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "Image target required",
                ));
            };
            let D2D_SIZE_U { width, height } = self.target.GetPixelSize();
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
            let mut file = Vec::with_capacity(54 + pixels.len());
            file.extend_from_slice(b"BM");
            file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
            file.extend_from_slice(&[0u8; 4]);
            file.extend_from_slice(&54u32.to_le_bytes());
            file.extend_from_slice(&40u32.to_le_bytes());
            file.extend_from_slice(&(width as i32).to_le_bytes());
            file.extend_from_slice(&(-(height as i32)).to_le_bytes());
            file.extend_from_slice(&1u16.to_le_bytes());
            file.extend_from_slice(&32u16.to_le_bytes());
            file.extend_from_slice(&[0u8; 24]);
            file.extend_from_slice(&pixels);
            std::fs::write(path, file)
                .map_err(|e| Error::new(windows::Win32::Foundation::E_FAIL, e.to_string()))
        }
    }
}
// Opaque material colours. Glass highlights are applied as a separate layer.
pub(crate) const FOCUS_FILL: u32 = 0x3b2633;
pub(crate) const WATER_FILL: u32 = 0x262626;
pub(crate) const TASK_FILL: u32 = 0x1b2521;
pub(crate) const TASK_PROGRESS_FILL: u32 = 0xff76e48b;
pub(crate) const TASK_ACCENT: u32 = 0x57fa65;
pub(crate) const SHELF_FILL: u32 = 0x443621;
fn material_fill(accent: Option<u32>) -> u32 {
    match accent.map(|c| c & 0xffffff) {
        Some(0xfa5f5f | 0xe0a3b0 | 0xe6a3af | FOCUS_FILL) => FOCUS_FILL,
        Some(0x818cf8 | 0xa5b4fc) => 0x45355f,
        Some(0x5fe0fa | 0x3de8ff | 0x38bdf8 | WATER_FILL) => WATER_FILL,
        Some(0x5ffa88 | 0x59daa2 | 0x75d6a0 | TASK_FILL | TASK_ACCENT) => TASK_FILL,
        Some(0xf59e0b | 0xfbbf24 | 0xeec16c | SHELF_FILL) => SHELF_FILL,
        _ => 0x1b2228,
    }
}
fn material_accent(accent: Option<u32>) -> u32 {
    match material_fill(accent) {
        FOCUS_FILL => 0xe6a3af,
        WATER_FILL => 0x5fe0fa,
        TASK_FILL => TASK_ACCENT,
        SHELF_FILL => 0xeec16c,
        0x45355f => 0xa5b4fc,
        _ => 0xffffff,
    }
}
pub fn palette(theme: usize) -> [u32; 3] {
    match theme {
        1 => [0x303a39, 0x252e2c, 0x171e1c],
        2 => [0x3a3531, 0x2c2825, 0x1b1917],
        _ => [0x2e3135, 0x222529, 0x141619],
    }
}
pub(super) fn wave_front_color(primary: u32) -> u32 {
    crate::motion::mix(primary, 0xffffff, 0.32)
}
fn color(hex: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 255) as f32 / 255.,
        g: ((hex >> 8) & 255) as f32 / 255.,
        b: (hex & 255) as f32 / 255.,
        a: if hex >> 24 == 0 {
            1.
        } else {
            (hex >> 24) as f32 / 255.
        },
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

pub fn wave_height(x: f64, span: f64, phase: f64, back: bool) -> f64 {
    if span <= 0. {
        return 0.;
    }
    let head = (x / 18.).clamp(0., 1.);
    let tail = ((span - x) / 18.).clamp(0., 1.);
    let envelope = head * head * (3. - 2. * head) * tail * tail * (3. - 2. * tail);
    let (w1, w2, w3, shift, a, b, c) = if back {
        (180., 280., 420., 1.3, 0.65, 0.4, 0.22)
    } else {
        (150., 240., 360., 0., 0.75, 0.48, 0.28)
    };
    let tau = std::f64::consts::TAU;
    12. * if back { 0.88 } else { 1. }
        * (0.54
            + 0.25 * (tau * x / w1 - phase * a - shift).sin()
            + 0.13 * (tau * x / w2 - phase * b - shift * 0.6 + 1.1).sin()
            + 0.08 * (tau * x / w3 - phase * c + 2.2).cos())
        .max(0.)
        * envelope
}
#[cfg(test)]
mod waveform_tests {
    use super::*;
    #[test]
    fn companion_glass_eyes_have_a_light_rim_and_neutral_silver_fill() {
        unsafe {
            let canvas = Canvas::image(65, 42).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&color(0x000000)));
            canvas.presence_eyes((0., 0., 65., 42.), crate::mascot::EyePose::default());
            canvas.target.EndDraw(None, None).unwrap();
            let mut pixels = [0; 65 * 42 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 65 * 4, &mut pixels)
                .unwrap();
            let pixel = |y: usize| {
                let start = (y * 65 + 23) * 4;
                &pixels[start..start + 3]
            };
            let (rim, body) = (pixel(18), pixel(23));
            assert!(rim[2] > body[2].saturating_add(40));
            assert!(body[2] > 60 && body[0] >= body[1] && body[1] >= body[2]);
        }
    }
    #[test]
    fn sleep_marks_appear_and_move_inside_compact_bounds() {
        unsafe {
            let canvas = Canvas::image(65, 42).unwrap();
            let mut pixels = [0; 65 * 42 * 4];
            let mut first_sleep_frame = [0; 65 * 17 * 4];
            for (sleep, sleep_phase) in [(0., 0.12), (1., 0.12), (1., 0.25)] {
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.presence_eyes(
                    (0., 0., 65., 42.),
                    crate::mascot::EyePose {
                        open: [0.; 2],
                        sleep,
                        sleep_phase,
                        ..Default::default()
                    },
                );
                canvas.target.EndDraw(None, None).unwrap();
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 65 * 4, &mut pixels)
                    .unwrap();
                let above_eyes = &pixels[..first_sleep_frame.len()];
                if sleep_phase == 0.25 {
                    assert_ne!(above_eyes, first_sleep_frame);
                } else if sleep > 0. {
                    first_sleep_frame.copy_from_slice(above_eyes);
                }
                for region in [39..44, 45..51, 53..61] {
                    let visible =
                        (3..17).any(|y| region.clone().any(|x| pixels[(y * 65 + x) * 4 + 2] > 48));
                    assert_eq!(visible, sleep == 1., "Sleep mark at {region:?}");
                }
            }
        }
    }
    #[test]
    fn clock_reflection_stays_in_the_glyphs_and_task_panels_stay_opaque() {
        unsafe {
            let canvas = Canvas::image(220, 80).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&D2D1_COLOR_F::default()));
            canvas.design_glass_clock("12.00", (20., 2., 147., 57.6));
            canvas.end().unwrap();
            let mut pixels = vec![0; 220 * 80 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 220 * 4, &mut pixels)
                .unwrap();
            let ink = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 245)
                .collect::<Vec<_>>();
            assert!(ink.len() > 300);
            assert!(
                ink.iter().filter(|p| p[2] > 210).count() > 20,
                "upper specular reflection"
            );
            assert!(
                ink.iter().filter(|p| p[2] < 150).count() > 20,
                "deeper lower glass face"
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % 220 >= 170)
                    .all(|(_, p)| p[3] == 0),
                "no clock background glow"
            );
            let canvas = Canvas::image(200, 160).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&D2D1_COLOR_F::default()));
            canvas.glass_panel((5., 5., 80., 150.), 10., 0x202022, 0.);
            canvas.glass_panel((105., 5., 90., 150.), 13., TASK_PROGRESS_FILL, 0.6);
            canvas.end().unwrap();
            let mut pixels = vec![0; 200 * 160 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 200 * 4, &mut pixels)
                .unwrap();
            let pixel = |x: usize, y: usize| &pixels[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4];
            let dark = pixel(45, 80);
            let green = pixel(150, 80);
            assert_eq!(dark[3], 255);
            assert_eq!(green[3], 255);
            assert!(
                (i16::from(dark[1]) - i16::from(dark[2])).abs() <= 3,
                "list is neutral dark glass"
            );
            for (channel, shift) in green.iter().zip([0, 8, 16]) {
                let fill = ((TASK_PROGRESS_FILL >> shift) & 255) as u8;
                assert!(channel.abs_diff(fill) <= 3, "solid green progress card");
            }
            assert!(
                pixel(150, 5)[2] > green[2] + 35,
                "visible progress-card rim"
            );
        }
    }
    #[test]
    fn play_ink_is_centered_inside_large_and_medium_buttons() {
        unsafe {
            for (scale, bounds) in [
                (1., (7., 7., 50., 50.)),
                (0.8, (7., 6., 50., 50.)),
                (1., (12., 12., 40., 39.)),
            ] {
                let canvas = Canvas::image(80, 80).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&D2D1_COLOR_F::default()));
                canvas.design_scale(scale);
                canvas.icon_centered("play", bounds, 24.);
                canvas.end().unwrap();
                let mut pixels = vec![0; 80 * 80 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 80 * 4, &mut pixels)
                    .unwrap();
                let mut ink = (80, 80, 0, 0);
                for y in 0..80 {
                    for x in 0..80 {
                        if pixels[(y * 80 + x) * 4 + 3] > 128 {
                            ink = (ink.0.min(x), ink.1.min(y), ink.2.max(x), ink.3.max(y));
                        }
                    }
                }
                assert!(ink.2 > ink.0 && ink.3 > ink.1);
                let center = (
                    (ink.0 + ink.2 + 1) as f32 / 2.,
                    (ink.1 + ink.3 + 1) as f32 / 2.,
                );
                assert!(
                    (center.0 - (bounds.0 + bounds.2 / 2.) * scale).abs() <= 0.75,
                    "horizontal ink center: {center:?}"
                );
                assert!(
                    (center.1 - (bounds.1 + bounds.3 / 2.)).abs() <= 0.75,
                    "vertical ink center: {center:?}"
                );
            }
        }
    }
    #[test]
    fn cover_glow_has_no_rectangular_cut_at_its_layout_bounds() {
        unsafe {
            let canvas = Canvas::image(680, 360).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&color(0x000000)));
            canvas.design_glow_at((0., 0., 680., 270.), 0x3b0808);
            canvas.end().unwrap();
            let mut pixels = vec![0; 680 * 360 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 680 * 4, &mut pixels)
                .unwrap();
            let red = |y: usize| i32::from(pixels[(y * 680 + 35) * 4 + 2]);
            assert!(
                red(269) > 5,
                "Fixture must reach a still-visible part of the glow"
            );
            assert!(
                (red(269) - red(270)).abs() <= 2,
                "Glow must fade across the old rectangular boundary"
            );
            assert!(
                red(300) < red(270),
                "Gradient must continue fading naturally"
            );
        }
    }
    #[test]
    fn lyric_glows_keep_cover_hues_move_and_reuse_brushes() {
        unsafe {
            let canvas = Canvas::image(680, 280).unwrap();
            let mut frames = Vec::new();
            for (palette, seconds) in [
                ([0x3b0808; 3], 0.),
                ([0x3b0808; 3], 8.),
                ([0x083b08; 3], 8.),
                ([0x3b3b3b; 3], 8.),
            ] {
                let old = canvas
                    .lyric_glow_brushes
                    .borrow()
                    .as_ref()
                    .map(|(colors, brushes)| (*colors, brushes[0].as_raw()));
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.lyric_glow((382., 0., 278., 280.), palette, seconds, 42, 1.);
                canvas.end().unwrap();
                if let Some((colors, ptr)) = old
                    && colors == palette
                {
                    assert_eq!(
                        ptr,
                        canvas.lyric_glow_brushes.borrow().as_ref().unwrap().1[0].as_raw()
                    );
                }
                let mut pixels = vec![0; 680 * 280 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 680 * 4, &mut pixels)
                    .unwrap();
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| p[..3].iter().any(|v| *v > 25)),
                    "glow must remain visible"
                );
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[..3].iter().all(|v| *v <= 110) && p[3] == 255),
                    "bound glow brightness behind lyrics"
                );
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| i % 680 < 65)
                        .all(|(_, p)| p[..3] == [0, 0, 0]),
                    "do not wash over the sidebar"
                );
                frames.push(pixels);
            }
            assert_ne!(frames[0], frames[1], "glows must move over time");
            let totals = |pixels: &[u8]| {
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .fold([0_u64; 3], |mut sum, p| {
                        for i in 0..3 {
                            sum[i] += u64::from(p[i]);
                        }
                        sum
                    })
            };
            let red = totals(&frames[1]);
            let green = totals(&frames[2]);
            assert!(red[2] > red[1] * 5 && red[2] > red[0] * 5);
            assert!(green[1] > green[2] * 5 && green[1] > green[0] * 5);
        }
    }
    #[test]
    fn compact_cover_and_context_share_one_reflective_glass_plate() {
        unsafe {
            let canvas = Canvas::image(48, 48).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&color(0x000000)));
            canvas.glass_content((9., 9., 29., 29.), 8., false, None, |b| {
                canvas.rect(b.0 + 1.2, b.1 + 1.2, b.2 - 2.4, b.3 - 2.4, 6.8, 0x334455);
            });
            canvas.end().unwrap();
            let mut pixels = vec![0; 48 * 48 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 48 * 4, &mut pixels)
                .unwrap();
            let pixel = |x: usize, y: usize| &pixels[(y * 48 + x) * 4..(y * 48 + x) * 4 + 4];
            assert!(
                pixel(23, 12)[2] > pixel(23, 28)[2],
                "reflection must cover artwork as well as the empty plate"
            );
            assert!(
                pixel(23, 22)[0] > pixel(23, 22)[2],
                "retain the artwork hue"
            );
            assert_eq!(pixel(9, 9), &[0, 0, 0, 255], "rounded outer corner");
            assert!(pixel(23, 9)[0] > pixel(23, 22)[0], "single thin upper rim");
        }
    }
    #[test]
    fn played_rails_use_the_front_wave_colour_at_both_thicknesses() {
        unsafe {
            let ink = 0x8649cb;
            let expected = crate::motion::mix(ink, 0xffffff, 0.32);
            for thickness in [2.5, 4.] {
                let canvas = Canvas::image(200, 30).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.mini_wave_tinted(
                    (10., 5., 180., 4.),
                    0.5,
                    0.,
                    false,
                    Some([ink; 2]),
                    thickness,
                );
                canvas.target.EndDraw(None, None).unwrap();
                let mut pixels = vec![0; 200 * 30 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 200 * 4, &mut pixels)
                    .unwrap();
                let p = &pixels[(11 * 200 + 30) * 4..(11 * 200 + 30) * 4 + 3];
                for (value, shift) in p.iter().zip([0, 8, 16]) {
                    assert!((*value as i32 - ((expected >> shift) & 255) as i32).abs() <= 2);
                }
                let unplayed = &pixels[(11 * 200 + 150) * 4..(11 * 200 + 150) * 4 + 3];
                assert!(unplayed.iter().any(|value| *value < 100));
            }
        }
    }
    #[test]
    fn tinted_wave_crests_cross_and_show_both_cover_colors() {
        unsafe {
            let primary = 0xcd368b;
            let secondary = 0xd92924;
            let front = crate::motion::mix(primary, 0xffffff, 0.32);
            let back = crate::motion::mix(secondary, 0xffffff, 0.12);
            let canvas = Canvas::image(200, 24).unwrap();
            let mut pixels = [0; 200 * 24 * 4];
            for amplitude in [6., 14.] {
                for seconds in [0., 1.5, 5., 30.] {
                    canvas.target.BeginDraw();
                    canvas.target.Clear(Some(&color(0x000000)));
                    canvas.mini_wave_tinted(
                        (10., 10., 180., amplitude),
                        0.72,
                        seconds,
                        true,
                        Some([primary, secondary]),
                        4.,
                    );
                    canvas.target.EndDraw(None, None).unwrap();
                    canvas
                        .bitmap
                        .as_ref()
                        .unwrap()
                        .CopyPixels(std::ptr::null(), 200 * 4, &mut pixels)
                        .unwrap();
                    for y in 0..15 {
                        for x in (12..18).chain(133..138) {
                            let start = (y * 200 + x) * 4;
                            assert_eq!(
                                &pixels[start..start + 3],
                                &[0; 3],
                                "Wave rises above the rail near an endpoint at ({x}, {y})"
                            );
                        }
                    }
                    let mut leading = [0; 2];
                    for x in 12..138 {
                        for y in 0..15 {
                            let start = (y * 200 + x) * 4;
                            let pixel = &pixels[start..start + 3];
                            if let Some(layer) = [back, front].into_iter().position(|ink| {
                                [0, 8, 16].into_iter().zip(pixel).all(|(shift, value)| {
                                    u32::from(*value).abs_diff((ink >> shift) & 255) <= 2
                                })
                            }) {
                                leading[layer] += 1;
                                break;
                            }
                        }
                    }
                    assert!(
                        leading.into_iter().all(|count| count >= 12),
                        "One layer dominates at {amplitude}px, {seconds}s: {leading:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn karaoke_ink_advances_and_stays_inside_wrapped_and_idle_viewports() {
        unsafe {
            let line = crate::lyrics::Line {
                at: 0.,
                end: Some(8.),
                text: "A long lyric that must remain inside its own reading area while it moves"
                    .into(),
                ..Default::default()
            };
            for scroll in [false, true] {
                let canvas = Canvas::image(240, 180).unwrap();
                let mut bright = Vec::new();
                for position in [0., 4., 8.] {
                    canvas.target.BeginDraw();
                    canvas.target.Clear(Some(&color(0x000000)));
                    canvas.design_karaoke(
                        &line,
                        (position, 8.),
                        (20., 10., 175., 150.),
                        (15., 700, 2),
                        scroll,
                        1.,
                    );
                    canvas.target.EndDraw(None, None).unwrap();
                    let mut pixels = vec![0; 240 * 180 * 4];
                    canvas
                        .bitmap
                        .as_ref()
                        .unwrap()
                        .CopyPixels(std::ptr::null(), 240 * 4, &mut pixels)
                        .unwrap();
                    let mut count = 0;
                    for y in 0..180 {
                        for x in 0..240 {
                            let pixel = &pixels[(y * 240 + x) * 4..(y * 240 + x) * 4 + 3];
                            if pixel.iter().all(|v| *v > 210) {
                                count += 1;
                            }
                            if !(20..195).contains(&x) || !(10..160).contains(&y) {
                                assert!(
                                    pixel.iter().all(|v| *v <= 2),
                                    "Lyric escaped its viewport"
                                );
                            }
                        }
                    }
                    bright.push(count);
                }
                assert_eq!(bright[0], 0);
                assert!(bright[1] > 0);
                assert!(bright[2] > 0);
                if !scroll {
                    assert!(bright[2] > bright[1]);
                }
            }
        }
    }
    #[test]
    fn compact_wave_touches_the_bottom_without_a_rail() {
        unsafe {
            let canvas = Canvas::image(270, 42).unwrap();
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&color(0x000000)));
            canvas.mini_wave_tinted(
                (54., 37., 162., 9.),
                0.5,
                5.,
                true,
                Some([0x32cc88, 0xe057a7]),
                0.,
            );
            canvas.target.EndDraw(None, None).unwrap();
            let mut pixels = vec![0; 270 * 42 * 4];
            canvas
                .bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 270 * 4, &mut pixels)
                .unwrap();
            let bottom = |x: usize| {
                let offset = (41 * 270 + x) * 4;
                &pixels[offset..offset + 3]
            };
            assert_ne!(bottom(95), &[0, 0, 0]);
            assert_eq!(bottom(56), &[0, 0, 0]);
            assert!((135..214).all(|x| bottom(x) == [0, 0, 0]));
        }
    }
    #[test]
    fn tinted_idle_wave_does_not_pass_the_playhead_at_track_start() {
        unsafe {
            for progress in [0., 0.005, 0.03, 0.1, 0.5, 1.] {
                let canvas = Canvas::image(220, 24).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.mini_wave_tinted(
                    (20., 5., 179., 6.),
                    progress,
                    5.,
                    true,
                    Some([0x32cc88, 0xe057a7]),
                    2.5,
                );
                canvas.target.EndDraw(None, None).unwrap();
                let mut pixels = vec![0; 220 * 24 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 220 * 4, &mut pixels)
                    .unwrap();
                let first_unplayed = (22. + 175. * progress).ceil() as usize + 1;
                for y in 0..10 {
                    for x in first_unplayed..199 {
                        let p = &pixels[(y * 220 + x) * 4..(y * 220 + x) * 4 + 3];
                        let chroma = p.iter().max().unwrap() - p.iter().min().unwrap();
                        assert!(
                            chroma <= 8,
                            "Coloured wave ahead of playhead at {progress}: ({x},{y})"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn shelf_preview_keeps_aspect_ratio_and_neutral_margins() {
        unsafe {
            for (width, height, inside, margin) in
                [(80, 40, (20, 50), (50, 20)), (40, 80, (50, 20), (20, 50))]
            {
                let canvas = Canvas::image(100, 100).unwrap();
                let image = crate::artwork::PreviewImage {
                    width,
                    height,
                    pixels: [0, 0, 255, 255].repeat((width * height) as usize),
                };
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x11171d)));
                canvas
                    .image_preview("fixture", &image, 10., 10., 80., 80., 7.)
                    .unwrap();
                canvas.target.EndDraw(None, None).unwrap();
                let mut pixels = vec![0; 100 * 100 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 400, &mut pixels)
                    .unwrap();
                let pixel =
                    |(x, y): (usize, usize)| &pixels[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
                assert_eq!(pixel(inside), &[0, 0, 255, 255]);
                assert_eq!(pixel(margin), &[0x1d, 0x17, 0x11, 255]);
            }
        }
    }
    #[test]
    fn fresh_surface_is_dark_before_its_first_content_frame() {
        unsafe {
            let canvas = Canvas::image(16, 16).expect("canvas");
            // Priming a surface must not make it eligible for content snapshots.
            assert!(!canvas.has_frame());
            assert!(canvas.snapshot().is_err());
            let mut pixels = vec![0u8; 16 * 16 * 4];
            canvas
                .bitmap
                .as_ref()
                .expect("bitmap")
                .CopyPixels(std::ptr::null(), 16 * 4, &mut pixels)
                .expect("pixels");
            for pixel in pixels.as_chunks::<4>().0 {
                assert_eq!(pixel, &[0x0c, 0x0a, 0x09, 0xff]);
            }
            canvas.begin_palette(palette(0));
            canvas.end().expect("first frame");
            assert!(canvas.has_frame());
            assert!(canvas.snapshot().is_ok());
        }
    }
    #[test]
    fn interaction_cache_is_bounded_and_motion_setting_stops_frames() {
        unsafe {
            let canvas = Canvas::image(32, 32).expect("canvas");
            canvas.set_glass_motion(true);
            for x in 0..1200 {
                canvas.hover_amount((x as f32, 0., 20., 20.), false);
            }
            assert!(canvas.glass_states.borrow().len() <= 512);
            assert!(!canvas.glass_animating());
            assert_eq!(canvas.hover_amount((1199., 0., 20., 20.), true), 0.);
            assert!(canvas.glass_animating());
            canvas.set_glass_motion(false);
            assert_eq!(canvas.hover_amount((1199., 0., 20., 20.), true), 1.);
            assert!(!canvas.glass_animating());
        }
    }
    #[test]
    fn island_mask_rounds_the_top_corners_without_losing_content() {
        unsafe {
            let canvas = Canvas::image(680, 280).unwrap();
            for (width, height, radius) in [(270., 50., 37.), (591., 101., 37.), (680., 280., 32.)]
            {
                let offset = (680. - width) / 2.;
                canvas
                    .begin_shell_palette(
                        [0x203040; 3],
                        ShellOutline {
                            floating: 1.,
                            width,
                            height,
                            radius,
                            shape: 1.,
                            offset,
                        },
                    )
                    .unwrap();
                canvas.rect(0., 0., 680., 280., 0., 0x203040);
                canvas.end().unwrap();
                let mut pixels = vec![0; 680 * 280 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 680 * 4, &mut pixels)
                    .unwrap();
                let alpha = |x: usize, y: usize| pixels[(y * 680 + x) * 4 + 3];
                assert_eq!(alpha(offset as usize, 0), 0);
                assert_eq!(alpha((offset + width - 1.) as usize, 0), 0);
                assert_eq!(alpha(340, 0), 255);
                assert_eq!(alpha(340, (height / 2.) as usize), 255);
                assert_eq!(alpha(offset as usize, (height - 1.) as usize), 0);
            }
        }
    }
    #[test]
    fn shell_mask_preserves_fractional_corner_coverage_and_opaque_content() {
        unsafe {
            let canvas = Canvas::image(680, 280).unwrap();
            for (width, height, shape, radius) in [
                (310., 50., 0., 37.),
                (591., 101., 1., 37.),
                (680., 280., 1., 32.),
                (631.25, 177.75, 0.73, 34.5),
            ] {
                let outline = ShellOutline {
                    floating: 0.,
                    width,
                    height,
                    shape,
                    radius,
                    offset: (680. - width) / 2.,
                };
                canvas.begin_shell_palette([0x203040; 3], outline).unwrap();
                // Child content cannot paint outside the shell mask.
                canvas.rect(0., 0., 680., 280., 0., 0x203040);
                canvas.end().unwrap();
                let mut pixels = vec![0; 680 * 280 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 680 * 4, &mut pixels)
                    .unwrap();
                let alpha = |x: usize, y: usize| pixels[(y * 680 + x) * 4 + 3];
                assert_eq!(alpha(340, 20), 255, "opaque center");
                assert_eq!(
                    alpha(outline.offset as usize, height as usize - 1),
                    0,
                    "transparent corner"
                );
                let mut partial = 0;
                let mut left_coverage = 0_i64;
                let mut right_coverage = 0_i64;
                for y in (height as usize - 25)..height.ceil() as usize {
                    for x in outline.offset as usize..(outline.offset + 70.) as usize {
                        let a = alpha(x, y);
                        if a > 0 && a < 255 {
                            partial += 1;
                        }
                        left_coverage += i64::from(a);
                        right_coverage += i64::from(alpha(679 - x, y));
                    }
                }
                assert!(
                    partial >= 12,
                    "rounded corner needs partial pixel coverage: {partial}"
                );
                // Software D2D quantizes coverage per edge direction; compare
                // the total mirrored area within two pixels, not identical ink.
                assert!(
                    (left_coverage - right_coverage).abs() <= 510,
                    "asymmetric corner area: {left_coverage} vs {right_coverage}"
                );
                assert!(!canvas.shell_layer.get());
            }
        }
    }
    #[test]
    fn monitor_badges_keep_fractional_edge_coverage_at_scaled_dpi() {
        unsafe {
            use crate::monitor::{Badges, Monitor};

            let outline = ShellOutline {
                floating: 1.,
                width: 250.,
                height: 34.,
                shape: 0.,
                radius: 37.,
                offset: 150.,
            };
            let badges = Badges::new(outline);
            let (x, y, w, h) = badges.memory;
            for dpi in [120, 144, 168, 192].map(crate::dpi::Scale::new) {
                let width = dpi.px(550) as usize;
                let height = dpi.px(43) as usize;
                let canvas = Canvas::image_at_dpi(width as u32, height as u32, dpi).unwrap();
                canvas.begin_shell_palette([0x203040; 3], outline).unwrap();
                canvas.finish_shell_clip();
                Monitor::default().draw_badges(&canvas, Badges::new(outline));
                canvas.end().unwrap();
                let frame = canvas.snapshot().unwrap();
                assert_eq!(frame.GetPixelSize().width as usize, width);
                assert!((frame.GetSize().width - 550.).abs() < 1.);

                let mut pixels = vec![0; width * height * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), (width * 4) as u32, &mut pixels)
                    .unwrap();
                let alpha = |x, y| pixels[(y * width + x) * 4 + 3];
                let center_y = dpi.physical(y + h / 2.).round() as usize;
                assert_eq!(alpha(dpi.physical(x - 3.).round() as usize, center_y), 0);
                assert_eq!(
                    alpha(dpi.physical(x + w / 2.).round() as usize, center_y),
                    255
                );
                let left = dpi.physical(x).floor() as usize;
                let top = dpi.physical(y).floor() as usize;
                assert_eq!(alpha(left, top), 0);
                let radius = dpi.physical(h / 2.).ceil() as usize;
                let partial = (top..top + radius)
                    .flat_map(|y| (left..left + radius).map(move |x| alpha(x, y)))
                    .filter(|&a| a > 0 && a < 255)
                    .count();
                assert!(
                    partial >= 10,
                    "rounded edge at DPI {}: {partial}",
                    dpi.dpi()
                );
            }
        }
    }

    #[test]
    #[ignore = "Creates two small temporary windows to sample DWM composition; run explicitly on an unlocked desktop"]
    fn desktop_compositor_preserves_shell_alpha() {
        let _buffered_paint = crate::input::BufferedPainting::new().unwrap();
        unsafe extern "system" fn edit_proc(
            hwnd: HWND,
            msg: u32,
            wp: windows::Win32::Foundation::WPARAM,
            lp: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe {
                let original =
                    std::mem::transmute::<isize, WNDPROC>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
                if msg == WM_ERASEBKGND {
                    return windows::Win32::Foundation::LRESULT(1);
                }
                if msg == WM_PAINT && crate::input::paint_opaque(hwnd, original) {
                    return windows::Win32::Foundation::LRESULT(0);
                }
                let result = CallWindowProcW(original, hwnd, msg, wp, lp);
                crate::input::refresh_opaque(hwnd, msg);
                result
            }
        }
        unsafe extern "system" fn wndproc(
            hwnd: HWND,
            msg: u32,
            wp: windows::Win32::Foundation::WPARAM,
            lp: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe {
                if msg == WM_CTLCOLOREDIT {
                    let dc = HDC(wp.0 as *mut _);
                    let background = if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 1 {
                        crate::input::FIELD_COLORREF
                    } else {
                        crate::input::TASK_FIELD_COLORREF
                    };
                    SetBkColor(dc, background);
                    SetTextColor(dc, crate::input::TEXT_COLORREF);
                    SetDCBrushColor(dc, background);
                    return windows::Win32::Foundation::LRESULT(
                        GetStockObject(DC_BRUSH).0 as isize,
                    );
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        use windows::Win32::{
            Graphics::{Dwm::*, Gdi::*},
            System::LibraryLoader::GetModuleHandleW,
            UI::{Controls::MARGINS, WindowsAndMessaging::*},
        };
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let background_brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0xff00ff));
            let class = w!("Horizon.AlphaProbe");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hbrBackground: background_brush,
                lpszClassName: class,
                ..Default::default()
            });
            let mut work = windows::Win32::Foundation::RECT::default();
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&mut work as *mut windows::Win32::Foundation::RECT).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .unwrap();
            let (x, y) = (work.left + 16, work.bottom - 116);
            let back = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class,
                w!("Horizon alpha test background"),
                WS_POPUP,
                x,
                y,
                100,
                100,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            let front = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
                class,
                w!("Horizon alpha test surface"),
                WS_POPUP | WS_CLIPCHILDREN,
                x + 10,
                y + 10,
                80,
                80,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            SetLayeredWindowAttributes(
                front,
                windows::Win32::Foundation::COLORREF(0),
                255,
                LWA_ALPHA,
            )
            .unwrap();
            DwmExtendFrameIntoClientArea(
                front,
                &MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                },
            )
            .unwrap();
            let guard = crate::layout::contour(82., 81., 1., 21.)
                .into_iter()
                .map(|(x, y)| windows::Win32::Foundation::POINT {
                    x: (x - 1.).round() as i32,
                    y: y.round() as i32,
                })
                .collect::<Vec<_>>();
            let region = CreatePolygonRgn(&guard, WINDING);
            assert_ne!(SetWindowRgn(front, Some(region), false), 0);
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE,
                30,
                30,
                20,
                20,
                Some(front),
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            let original = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            SetWindowLongPtrW(edit, GWLP_USERDATA, original);
            let canvas = Canvas::new(front, 80, 80, true, crate::dpi::Scale::STANDARD).unwrap();
            canvas
                .begin_shell_palette(
                    [0x203040; 3],
                    ShellOutline {
                        floating: 0.,
                        width: 80.,
                        height: 80.,
                        shape: 1.,
                        radius: 20.,
                        offset: 0.,
                    },
                )
                .unwrap();
            canvas.end().unwrap();
            let _ = ShowWindow(back, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(back);
            let _ = ShowWindow(front, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(edit);
            // Present after show, matching the retained HWND target's lifecycle.
            canvas
                .begin_shell_palette(
                    [0x203040; 3],
                    ShellOutline {
                        floating: 0.,
                        width: 80.,
                        height: 80.,
                        shape: 1.,
                        radius: 20.,
                        offset: 0.,
                    },
                )
                .unwrap();
            canvas.end().unwrap();
            DwmFlush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(150));
            let dc = GetDC(None);
            let corner = GetPixel(dc, x + 11, y + 88).0;
            let center = GetPixel(dc, x + 50, y + 30).0;
            let child = GetPixel(dc, x + 50, y + 50).0;
            let mut partial = 0;
            for py in 60..80 {
                for px in 0..25 {
                    let pixel = GetPixel(dc, x + 10 + px, y + 10 + py).0;
                    if pixel != 0xff00ff && pixel != 0x403020 && pixel != 0xffffffff {
                        partial += 1;
                    }
                }
            }
            SendMessageW(
                edit,
                WM_CHAR,
                Some(windows::Win32::Foundation::WPARAM('A' as usize)),
                Some(windows::Win32::Foundation::LPARAM(1)),
            );
            SendMessageW(
                edit,
                windows::Win32::UI::Controls::EM_SETSEL,
                Some(windows::Win32::Foundation::WPARAM(0)),
                Some(windows::Win32::Foundation::LPARAM(1)),
            );
            SendMessageW(
                edit,
                windows::Win32::UI::Controls::EM_REPLACESEL,
                Some(windows::Win32::Foundation::WPARAM(1)),
                Some(windows::Win32::Foundation::LPARAM(w!("B").0 as isize)),
            );
            let mut text = [0_u16; 8];
            let count = GetWindowTextW(edit, &mut text);
            DwmFlush().unwrap();
            let after_typing = GetPixel(dc, x + 58, y + 58).0;
            let mut letters = 0;
            for py in 40..59 {
                for px in 40..59 {
                    let pixel = GetPixel(dc, x + px, y + py).0;
                    if pixel & 255 > 180 && (pixel >> 8) & 255 > 180 && (pixel >> 16) & 255 > 180 {
                        letters += 1;
                    }
                }
            }
            SetWindowLongPtrW(front, GWLP_USERDATA, 1);
            SendMessageW(
                edit,
                WM_SETTEXT,
                None,
                Some(windows::Win32::Foundation::LPARAM(w!("").0 as isize)),
            );
            DwmFlush().unwrap();
            let calendar_field = GetPixel(dc, x + 50, y + 50).0;
            let _ = ReleaseDC(None, dc);
            drop(canvas);
            DestroyWindow(front).unwrap();
            DestroyWindow(back).unwrap();
            UnregisterClassW(class, Some(instance.into())).unwrap();
            let _ = DeleteObject(background_brush.into());
            assert_eq!(
                corner, 0xff00ff,
                "DWM must reveal the probe background at the transparent corner; center={center:06x}"
            );
            assert_eq!(center, 0x403020, "shell interior must remain opaque");
            assert_eq!(
                child,
                crate::input::TASK_FIELD_COLORREF.0,
                "dark native child edit must remain opaque"
            );
            assert_eq!(
                String::from_utf16_lossy(&text[..count as usize]),
                "B",
                "typing and replacement remain native"
            );
            assert!(
                letters > 4,
                "typed text must remain visible on the dark input: {letters}"
            );
            assert_eq!(
                after_typing,
                crate::input::TASK_FIELD_COLORREF.0,
                "input remains opaque after native edits"
            );
            assert_eq!(
                calendar_field,
                crate::input::FIELD_COLORREF.0,
                "calendar input stays opaque too"
            );
            assert!(
                partial >= 12,
                "DWM must preserve fractional edge pixels through the guarded Win32 region: {partial}"
            );
        }
    }
    #[test]
    fn music_toggle_marker_tracks_selected_state_without_hover() {
        unsafe {
            let canvas = Canvas::image(64, 64).unwrap();
            canvas.set_glass_motion(false);
            for active in [false, true, false] {
                canvas.begin_palette([0; 3]);
                canvas.glass_content((7., 7., 50., 50.), 17., false, Some(active), |_| {});
                canvas.end().unwrap();
                let mut pixels = vec![0; 64 * 64 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 64 * 4, &mut pixels)
                    .unwrap();
                assert_eq!(pixels[(52 * 64 + 32) * 4] > 190, active);
            }
        }
    }
    #[test]
    fn music_glass_is_opaque_and_its_reflection_survives_without_hover() {
        unsafe {
            let mut faces = Vec::new();
            for backdrop in [0x990000, 0x000099] {
                let canvas = Canvas::image(64, 64).unwrap();
                canvas.begin_palette([backdrop; 3]);
                assert_eq!(canvas.glass_button((7., 7., 50., 50.), 17., false), 0.);
                canvas.end().unwrap();
                let mut pixels = vec![0; 64 * 64 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 64 * 4, &mut pixels)
                    .unwrap();
                let pixel =
                    |x: usize, y: usize| pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].to_vec();
                assert_eq!(pixel(32, 32)[3], 255);
                assert!(
                    pixel(32, 7)[0] > pixel(32, 32)[0] + 20,
                    "visible upper rim while idle"
                );
                assert!(
                    pixel(32, 13)[0] > pixel(32, 40)[0],
                    "face reflection fades downwards"
                );
                faces.push(pixel(32, 32));
            }
            assert_eq!(
                faces[0], faces[1],
                "wallpaper must not tint the solid button face"
            );
            for (name, bytes) in [
                (
                    "media-pause",
                    include_bytes!("../assets/figma/media-pause-glyph.png").as_slice(),
                ),
                (
                    "media-lyrics",
                    include_bytes!("../assets/figma/media-lyrics-glyph.png").as_slice(),
                ),
                (
                    "peek-pause",
                    include_bytes!("../assets/figma/peek-pause-glyph.png").as_slice(),
                ),
            ] {
                let canvas = Canvas::image(50, 50).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&D2D1_COLOR_F::default()));
                canvas
                    .design_asset(name, bytes, (0., 0., 50., 50.))
                    .unwrap();
                canvas.end().unwrap();
                let mut pixels = vec![0; 50 * 50 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 50 * 4, &mut pixels)
                    .unwrap();
                assert_eq!(
                    pixels[(25 * 50 + 4) * 4 + 3],
                    0,
                    "{name} still contains a baked-in plate"
                );
                assert!(
                    pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 240),
                    "glyph must remain visible"
                );
            }
        }
    }
    #[test]
    fn static_design_assets_keep_native_bitmap_dimensions() {
        unsafe {
            let canvas = Canvas::image(600, 400).unwrap();
            canvas.begin_palette(palette(0));
            canvas
                .design_asset(
                    "nav-music",
                    include_bytes!("../assets/figma/nav-music.png"),
                    (0., 0., 40., 40.),
                )
                .unwrap();
            canvas
                .design_asset(
                    "shell",
                    include_bytes!("../assets/figma/shell.png"),
                    (0., 0., 591., 369.),
                )
                .unwrap();
            canvas.end().unwrap();
            let assets = canvas.design_assets.borrow();
            let icon = assets["nav-music"].GetPixelSize();
            let shell = assets["shell"].GetPixelSize();
            assert_eq!((icon.width, icon.height), (40, 40));
            assert_eq!((shell.width, shell.height), (591, 369));
        }
    }
    #[test]
    fn design_measurement_wraps_paragraphs_and_cache_stays_bounded() {
        unsafe {
            let c = Canvas::image(100, 100).unwrap();
            let short = c.design_measure("Rapat", 175., (18., 700, 3)).1;
            let long = c
                .design_measure(
                    "Rapat koordinasi dan pembahasan desain aplikasi Horizon",
                    175.,
                    (18., 700, 3),
                )
                .1;
            assert!(long > short * 1.5);
            for i in 0..200 {
                c.design_measure(&format!("Judul agenda {i}"), 175., (12., 400, 3));
            }
            assert!(c.design_measurements.borrow().len() <= 128);
        }
    }
    #[test]
    fn waveform_tapers_and_stays_in_bounds() {
        for back in [true, false] {
            assert_eq!(wave_height(0., 200., 2., back), 0.);
            assert_eq!(wave_height(200., 200., 2., back), 0.);
            for x in 0..=200 {
                assert!((0.0..=12.).contains(&wave_height(x as f64, 200., 2., back)));
            }
        }
    }

    #[test]
    fn heart_icon_filled_draws_interior_pixels() {
        unsafe {
            let canvas = Canvas::image(32, 32).expect("Canvas creation");
            canvas.target.BeginDraw();
            canvas.target.Clear(Some(&color(0x000000)));
            canvas.icon_filled("heart", 4., 4., 24., 0xff4272);
            canvas.target.EndDraw(None, None).expect("EndDraw");
            let bitmap = canvas.bitmap.as_ref().expect("bitmap");
            let mut pixels = vec![0u8; (32 * 32 * 4) as usize];
            bitmap
                .CopyPixels(std::ptr::null(), 32 * 4, &mut pixels)
                .expect("CopyPixels");
            // Center interior of 24x24 heart at offset (4,4) is around (16, 14)
            let offset = (14 * 32 + 16) * 4;
            let b = pixels[offset];
            let g = pixels[offset + 1];
            let r = pixels[offset + 2];
            assert!(
                r > 200,
                "Heart interior must be filled with red (got r={r}, g={g}, b={b})"
            );

            // Also test unfilled icon leaves center empty
            let canvas_unfilled = Canvas::image(32, 32).expect("Canvas creation");
            canvas_unfilled.target.BeginDraw();
            canvas_unfilled.target.Clear(Some(&color(0x000000)));
            canvas_unfilled.icon("heart", 4., 4., 24., 0xcbd2d9);
            canvas_unfilled.target.EndDraw(None, None).expect("EndDraw");
            let bitmap2 = canvas_unfilled.bitmap.as_ref().expect("bitmap");
            let mut pixels2 = vec![0u8; (32 * 32 * 4) as usize];
            bitmap2
                .CopyPixels(std::ptr::null(), 32 * 4, &mut pixels2)
                .expect("CopyPixels");
            let r2 = pixels2[offset + 2];
            assert_eq!(r2, 0, "Heart interior must be hollow/empty when not filled");
        }
    }
}
