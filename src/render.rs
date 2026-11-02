use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
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
thread_local! {static FACTORY:OnceCell<ID2D1Factory>=const{OnceCell::new()};static FONTS:OnceCell<IDWriteFontCollection>=const{OnceCell::new()};static ICONS:RefCell<HashMap<&'static str,ID2D1Geometry>>=RefCell::new(HashMap::new());}
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
pub struct Canvas {
    target: ID2D1RenderTarget,
    write: IDWriteFactory,
    fonts: IDWriteFontCollection,
    formats: RefCell<HashMap<(u16, bool, u8), IDWriteTextFormat>>,
    design_formats: RefCell<HashMap<(u16, u16, u8), IDWriteTextFormat>>,
    karaoke_layout: RefCell<Option<KaraokeLayout>>,
    #[allow(clippy::type_complexity)]
    design_measurements: RefCell<std::collections::VecDeque<(String, u16, u16, u16, u8, f32, f32)>>,
    design_assets: RefCell<HashMap<&'static str, ID2D1Bitmap>>,
    bitmap: Option<IWICBitmap>,
    width: u32,
    height: u32,
    presented: Cell<bool>,
    cover_cache: RefCell<Option<(String, ID2D1Bitmap)>>,
    preview_cache: RefCell<Option<(String, ID2D1Bitmap)>>,
    solid_brush: ID2D1SolidColorBrush,
    stroke: ID2D1StrokeStyle,
    glass_motion: Cell<bool>,
    interaction_pressed: Cell<bool>,
    #[allow(clippy::type_complexity)]
    glass_states: RefCell<HashMap<(i32, i32, i32, i32, u8), crate::motion::Tween>>,
}
impl Canvas {
    pub unsafe fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self> {
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
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                        ..Default::default()
                    },
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd,
                        pixelSize: D2D_SIZE_U { width, height },
                        presentOptions: D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS,
                    },
                )?
                .cast()?;
            Self::from_target(target, None, width, height)
        }
    }
    pub unsafe fn image(width: u32, height: u32) -> Result<Self> {
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
                    ..Default::default()
                },
            )?;
            Self::from_target(target, Some(bitmap), width, height)
        }
    }
    unsafe fn from_target(
        target: ID2D1RenderTarget,
        bitmap: Option<IWICBitmap>,
        width: u32,
        height: u32,
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
                karaoke_layout: RefCell::new(None),
                design_measurements: RefCell::new(std::collections::VecDeque::new()),
                design_assets: RefCell::new(HashMap::new()),
                presented: Cell::new(false),
                glass_motion: Cell::new(false),
                glass_states: RefCell::new(HashMap::new()),
                interaction_pressed: Cell::new(false),
                cover_cache: RefCell::new(None),
                preview_cache: RefCell::new(None),
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
                width,
                height,
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
            let base_color =
                crate::motion::mix(0x141822, material_fill(accent), light.max(selection));
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
    pub fn set_interaction_pressed(&self, pressed: bool) {
        self.interaction_pressed.set(pressed);
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
    pub unsafe fn glass_edge(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, hover: f32) {
        unsafe {
            // A white hairline supports the thicker top reflection, fading
            // around the corners to a thin lower edge. No dark bevel or wash.
            for layer in 0..2 {
                let alpha = (64. + hover * 16.) as u32;
                let (stroke, colors) = match layer {
                    0 => (0.4, [0x30ffffff, 0x14ffffff, 0x28ffffff]),
                    _ => (1.3, [(alpha << 24) | 0xffffff, 0x0cffffff, 0x01ffffff]),
                };
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
                D2D_SIZE_U {
                    width: self.width,
                    height: self.height,
                },
                None,
                0,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: self.target.GetPixelFormat(),
                    dpiX: 96.,
                    dpiY: 96.,
                },
            )?;
            bitmap.CopyFromRenderTarget(None, &self.target, None)?;
            Ok(bitmap)
        }
    }
    pub fn has_frame(&self) -> bool {
        self.presented.get()
    }
    pub unsafe fn overlay(&self, bitmap: &ID2D1Bitmap, opacity: f32, shell: bool) {
        unsafe {
            let x = if shell { 0. } else { 62. };
            let y = if shell { 0. } else { 14. };
            let bounds = rect(x, y, self.width as f32 - x, self.height as f32 - y);
            self.target.DrawBitmap(
                bitmap,
                Some(&bounds),
                opacity,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                Some(&bounds),
            );
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
                        Y: self.height as f32,
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
                    center: Vector2 {
                        X: x + 35_f32.min(width * 0.08),
                        Y: y + 35_f32.min(height * 0.4),
                    },
                    gradientOriginOffset: Vector2 { X: 0., Y: 0. },
                    radiusX: width * 0.65,
                    radiusY: height * 1.1,
                },
                None,
                &stops,
            ) {
                self.target
                    .FillRectangle(&rect(x, y, width, height), &brush);
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
                let bitmap = self.target.CreateBitmap(
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
                )?;
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
        tint: Option<u32>,
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

            // 1. Background unplayed rail
            self.rect(
                x + inset,
                rail_y,
                length,
                rail_h,
                rail_h / 2.,
                tint.map_or(0x485057, |ink| crate::motion::mix(ink, 0x14161a, 0.68)),
            );

            // 2. Liquid wave animation
            if active_span > 4. {
                let samples = (active_span / 2.).ceil().max(16.) as usize;
                let phase = if moving || tint.is_some() {
                    seconds * 3.6
                } else {
                    0.
                };

                for layer in 0..2 {
                    let back = layer == 0;
                    let layer_amp = amplitude as f64 * if back { 0.88 } else { 1. };
                    let taper_dist = 8.0f64;
                    let w1 = if back { 85.0 } else { 70.0 };
                    let w2 = if back { 135.0 } else { 110.0 };
                    let w3 = if back { 200.0 } else { 160.0 };
                    let p_speed1 = if back { 0.65 } else { 0.75 };
                    let p_speed2 = if back { 0.40 } else { 0.48 };
                    let p_speed3 = if back { 0.22 } else { 0.28 };
                    let p_shift = if back { 1.3 } else { 0.0 };

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
                            let t_head = (u / taper_dist).clamp(0., 1.);
                            let t_tail = (((wave_end - cur_x) as f64) / taper_dist).clamp(0., 1.);
                            let env = (t_head * t_head * (3. - 2. * t_head))
                                * (t_tail * t_tail * (3. - 2. * t_tail));

                            let s1 = (2. * std::f64::consts::PI * u / w1
                                - (phase * p_speed1 + p_shift))
                                .sin();
                            let s2 = (2. * std::f64::consts::PI * u / w2
                                - (phase * p_speed2 + p_shift * 0.6)
                                + 1.1)
                                .sin();
                            let s3 = (2. * std::f64::consts::PI * u / w3 - (phase * p_speed3)
                                + 2.2)
                                .cos();

                            let elevation = (0.54 + 0.25 * s1 + 0.13 * s2 + 0.08 * s3).max(0.);
                            let h = (layer_amp * elevation * env) as f32;

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
                                &tint.map_or(
                                    D2D1_COLOR_F {
                                        r: 0.84,
                                        g: 0.88,
                                        b: 0.92,
                                        a: if back { 0.45 } else { 0.82 },
                                    },
                                    |ink| {
                                        color(crate::motion::mix(
                                            ink,
                                            0xffffff,
                                            if back { 0.08 } else { 0.32 },
                                        ))
                                    },
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
            if bar_w > 0. {
                self.rect(
                    x + inset,
                    rail_y,
                    bar_w,
                    rail_h,
                    rail_h / 2.,
                    tint.map_or(0xd0d7de, |ink| crate::motion::mix(ink, 0xffffff, 0.32)),
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
    pub unsafe fn end(&self) -> Result<()> {
        unsafe {
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
    pub unsafe fn compact_status(
        &self,
        b: (f32, f32, f32, f32),
        hover: f32,
        battery: crate::power::Battery,
        phase: f32,
    ) {
        unsafe {
            self.ellipse(
                b.0,
                b.1,
                b.2,
                b.3,
                crate::motion::mix(0x333333, 0x404040, hover),
            );
            if let Ok(brush) = self.brush(0x686868) {
                self.target.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 {
                            X: b.0 + b.2 / 2.,
                            Y: b.1 + b.3 / 2.,
                        },
                        radiusX: (b.2 - 0.7) / 2.,
                        radiusY: (b.3 - 0.7) / 2.,
                    },
                    &brush,
                    0.7,
                    None,
                );
            }
            let points = [
                (9., 0.),
                (0., 9.),
                (6., 9.),
                (3., 16.),
                (13., 6.),
                (7., 6.),
                (9., 0.),
            ];
            let mapped: Vec<_> = points
                .iter()
                .map(|v| (b.0 + 10. + v.0, b.1 + 7. + v.1))
                .collect();
            if battery.energized() {
                let alpha = (20. + 18. * (phase * 3.).sin().abs()) as u32;
                self.ellipse(b.0 + 4., b.1 + 3., 24., 24., (alpha << 24) | 0x57fa65);
            }
            self.polygon(&mapped, 0x171b19);
            if let Some(percent) = battery.percent {
                let height = 16. * percent as f32 / 100.;
                if height > 0. {
                    self.push_design_clip((b.0 + 8., b.1 + 23. - height, 18., height));
                    self.polygon(&mapped, 0x57fa65);
                    self.pop_design_clip();
                }
            }
            for pair in points.windows(2) {
                let p = |v: (f32, f32)| (b.0 + 10. + v.0, b.1 + 7. + v.1);
                self.line(
                    p(pair[0]),
                    p(pair[1]),
                    0.85,
                    if battery.percent.is_some() {
                        0xc8eed3
                    } else {
                        0x929a96
                    },
                );
            }
            if battery.energized() {
                for (index, (x, y)) in [(6., 9.), (26., 7.), (25., 24.)].into_iter().enumerate() {
                    let amount = (phase * 2.4 + index as f32 * 2.1).sin().max(0.);
                    let radius = 0.4 + amount * 1.5;
                    let point = (b.0 + x, b.1 + y);
                    self.line(
                        (point.0 - radius, point.1),
                        (point.0 + radius, point.1),
                        0.8,
                        0xd3ffdc,
                    );
                    self.line(
                        (point.0, point.1 - radius),
                        (point.0, point.1 + radius),
                        0.8,
                        0xd3ffdc,
                    );
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
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: scale,
                M22: 1.,
                M31: offset,
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
                1 | 2 => DWRITE_TEXT_ALIGNMENT_CENTER,
                4 => DWRITE_TEXT_ALIGNMENT_TRAILING,
                _ => DWRITE_TEXT_ALIGNMENT_LEADING,
            })?;
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
    pub unsafe fn design_karaoke(
        &self,
        line: &crate::lyrics::Line,
        timing: (f64, f64),
        b: (f32, f32, f32, f32),
        style: (f32, u16, u8),
        scroll: bool,
    ) {
        if line.text.is_empty() {
            return;
        }
        unsafe {
            let width_key = (b.2 * 10.).round() as u32;
            let style_key = ((style.0 * 10.) as u16, style.1, style.2);
            let layout = (|| -> Result<IDWriteTextLayout> {
                let mut cache = self.karaoke_layout.borrow_mut();
                if let Some(old) = &*cache
                    && old.text == line.text
                    && old.width == width_key
                    && old.style == style_key
                    && old.scroll == scroll
                {
                    return Ok(old.layout.clone());
                }
                let format =
                    self.design_format((style.0, style.1, if scroll { 0 } else { style.2 }))?;
                let layout = self.write.CreateTextLayout(
                    &line.text.encode_utf16().collect::<Vec<_>>(),
                    &format,
                    if scroll { 10000. } else { b.2.max(1.) },
                    10000.,
                )?;
                *cache = Some(KaraokeLayout {
                    text: line.text.clone(),
                    width: width_key,
                    style: style_key,
                    scroll,
                    layout: layout.clone(),
                });
                Ok(layout)
            })();
            let Ok(layout) = layout else {
                self.design_text(&line.text, b, style, 0xffffff);
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
            if let Ok(brush) = self.brush(0x828b98) {
                self.target
                    .DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
            if units > 0.
                && let Ok(brush) = self.brush(0xffffff)
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
    pub unsafe fn toggle(&self, x: f32, y: f32, on: bool) {
        unsafe {
            if on {
                self.rect(x, y, 42., 24., 12., 0xffffff);
                self.rect(x + 21., y + 3., 18., 18., 9., 0x0b0c0e);
            } else {
                self.rect(x, y, 42., 24., 12., 0x1c1f24);
                self.outline(x, y, 42., 24., 12., 0x2e333b);
                self.rect(x + 3., y + 3., 18., 18., 9., 0x6e7681);
            }
        }
    }
    pub unsafe fn icon(&self, name: &'static str, x: f32, y: f32, size: f32, _c: u32) {
        unsafe {
            let result = ICONS.with(|icons| -> Result<ID2D1Geometry> {
                if let Some(g) = icons.borrow().get(name) {
                    return Ok(g.clone());
                }
                let g = crate::icons::outline(&factory()?, name)?;
                icons.borrow_mut().insert(name, g.clone());
                Ok(g)
            });
            if let (Ok(g), Ok(b)) = (result, self.brush(0xffffffff)) {
                let scale = size / 24.;
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
    pub unsafe fn icon_filled(&self, name: &'static str, x: f32, y: f32, size: f32, _c: u32) {
        unsafe {
            let actual_name = match name {
                "heart" => "heart-fill",
                other => other,
            };
            let result = ICONS.with(|icons| -> Result<ID2D1Geometry> {
                if let Some(g) = icons.borrow().get(actual_name) {
                    return Ok(g.clone());
                }
                let g = crate::icons::outline(&factory()?, actual_name)?;
                icons.borrow_mut().insert(actual_name, g.clone());
                Ok(g)
            });
            if let (Ok(g), Ok(b)) = (result, self.brush(0xffffffff)) {
                let scale = size / 24.;
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
    pub unsafe fn save_bmp(&self, path: &Path) -> Result<()> {
        unsafe {
            let Some(bitmap) = &self.bitmap else {
                return Err(Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "Image target required",
                ));
            };
            let mut pixels = vec![0u8; (self.width * self.height * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), self.width * 4, &mut pixels)?;
            let mut file = Vec::with_capacity(54 + pixels.len());
            file.extend_from_slice(b"BM");
            file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
            file.extend_from_slice(&[0u8; 4]);
            file.extend_from_slice(&54u32.to_le_bytes());
            file.extend_from_slice(&40u32.to_le_bytes());
            file.extend_from_slice(&(self.width as i32).to_le_bytes());
            file.extend_from_slice(&(-(self.height as i32)).to_le_bytes());
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
    fn played_rails_use_the_front_wave_colour_at_both_thicknesses() {
        unsafe {
            let ink = 0x8649cb;
            let expected = crate::motion::mix(ink, 0xffffff, 0.32);
            for thickness in [2.5, 4.] {
                let canvas = Canvas::image(200, 30).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.mini_wave_tinted((10., 5., 180., 4.), 0.5, 0., false, Some(ink), thickness);
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
    fn compact_battery_fill_uses_the_reported_percentage() {
        unsafe {
            let mut counts = Vec::new();
            for percent in [0, 50, 100] {
                let canvas = Canvas::image(80, 50).unwrap();
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.compact_status(
                    (20., 10., 32., 30.),
                    0.,
                    crate::power::Battery {
                        percent: Some(percent),
                        charging: false,
                        plugged: Some(false),
                    },
                    0.,
                );
                canvas.target.EndDraw(None, None).unwrap();
                let mut pixels = vec![0; 80 * 50 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 80 * 4, &mut pixels)
                    .unwrap();
                counts.push(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|p| {
                            p[1] as i16 - p[0] as i16 > 60 && p[1] as i16 - p[2] as i16 > 60
                        })
                        .count(),
                );
            }
            assert_eq!(counts[0], 0);
            assert!(counts[1] > 0 && counts[2] > counts[1]);
        }
    }
    #[test]
    fn connected_adapter_changes_the_bolt_even_when_not_charging() {
        unsafe {
            let canvas = Canvas::image(80, 50).unwrap();
            let mut frames = Vec::new();
            for plugged in [false, true] {
                canvas.target.BeginDraw();
                canvas.target.Clear(Some(&color(0x000000)));
                canvas.compact_status(
                    (20., 10., 32., 30.),
                    0.,
                    crate::power::Battery {
                        percent: Some(80),
                        charging: false,
                        plugged: Some(plugged),
                    },
                    0.5,
                );
                canvas.target.EndDraw(None, None).unwrap();
                let mut pixels = vec![0; 80 * 50 * 4];
                canvas
                    .bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 80 * 4, &mut pixels)
                    .unwrap();
                frames.push(pixels);
            }
            assert_ne!(frames[0], frames[1]);
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
                    Some(0x32cc88),
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
