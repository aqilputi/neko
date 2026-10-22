//! Test harness: a GLES 3.0 context on a surfaceless EGL display (no
//! window, no compositor, no /dev/dri needed with swrast), a caller-owned
//! FBO to draw into, and PNG goldens. `golden.rs` includes it as a module;
//! on its own it only checks that the context comes up.
#![allow(dead_code)]

use std::ffi::c_void;
use std::path::PathBuf;

use khronos_egl as egl;
use nkw_gl::glow::{self, HasContext};

const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const OPENGL_ES3_BIT: egl::Int = 0x40;
const GL_TEXTURE_2D_KHR: egl::Enum = 0x30B1;

pub struct Ctx {
    pub gl: glow::Context,
    pub display: egl::Display,
    pub context: egl::Context,
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let e = &egl::API;
        let _ = e.make_current(self.display, None, None, None);
        let _ = e.destroy_context(self.display, self.context);
    }
}

/// A current GLES 3.0 context on this thread, or `None` (with the reason
/// on stderr) when this machine has no usable surfaceless EGL.
pub fn context() -> Option<Ctx> {
    match try_context() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("SKIP: no surfaceless EGL/GLES 3.0 context here ({e})");
            None
        }
    }
}

fn try_context() -> Result<Ctx, String> {
    let e = &egl::API;
    let display = unsafe {
        e.get_platform_display(
            PLATFORM_SURFACELESS_MESA,
            egl::DEFAULT_DISPLAY,
            &[egl::ATTRIB_NONE],
        )
    }
    .map_err(|err| format!("eglGetPlatformDisplay: {err}"))?;
    e.initialize(display)
        .map_err(|err| format!("eglInitialize: {err}"))?;
    e.bind_api(egl::OPENGL_ES_API)
        .map_err(|err| format!("eglBindAPI: {err}"))?;
    let config = e
        .choose_first_config(
            display,
            &[
                egl::RENDERABLE_TYPE,
                OPENGL_ES3_BIT,
                egl::SURFACE_TYPE,
                egl::PBUFFER_BIT,
                egl::NONE,
            ],
        )
        .map_err(|err| format!("eglChooseConfig: {err}"))?
        .ok_or("no GLES 3 config")?;
    let context = e
        .create_context(
            display,
            config,
            None,
            &[
                egl::CONTEXT_MAJOR_VERSION,
                3,
                egl::CONTEXT_MINOR_VERSION,
                0,
                egl::NONE,
            ],
        )
        .map_err(|err| format!("eglCreateContext: {err}"))?;
    e.make_current(display, None, None, Some(context))
        .map_err(|err| format!("eglMakeCurrent: {err}"))?;
    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            e.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const c_void)
        })
    };
    Ok(Ctx {
        gl,
        display,
        context,
    })
}

impl Ctx {
    pub fn renderer(&self) -> String {
        unsafe { self.gl.get_parameter_string(glow::RENDERER) }
    }

    pub fn has_ext(&self, name: &str) -> bool {
        self.gl.supported_extensions().contains(name)
    }

    /// An RGBA8 FBO of `size`, for `Target::Fbo`.
    pub fn fbo(&self, size: [u32; 2]) -> Fbo {
        unsafe {
            let gl = &self.gl;
            let rb = gl.create_renderbuffer().unwrap();
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::RGBA8,
                size[0] as i32,
                size[1] as i32,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(rb),
            );
            assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Fbo { fbo, rb, size }
        }
    }

    /// Reads `fbo` back as top-down RGBA8 rows (PNG order).
    pub fn read(&self, fbo: Option<glow::Framebuffer>, size: [u32; 2]) -> Vec<u8> {
        let (w, h) = (size[0] as usize, size[1] as usize);
        let mut px = vec![0u8; w * h * 4];
        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, fbo);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut px)),
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        flip_rows(&px, w)
    }

    /// Reads a 2D texture back by attaching it to a scratch FBO.
    pub fn read_texture(&self, id: u32, size: [u32; 2]) -> Vec<u8> {
        unsafe {
            let gl = &self.gl;
            let tex = glow::NativeTexture(std::num::NonZeroU32::new(id).unwrap());
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let px = self.read(Some(fbo), size);
            gl.delete_framebuffer(fbo);
            px
        }
    }

    /// Uploads top-down RGBA8 rows as a 2D texture (how a client buffer
    /// arrives: row 0 is the top of the image).
    pub fn texture(&self, rgba: &[u8], size: [u32; 2]) -> u32 {
        unsafe {
            let gl = &self.gl;
            let tex = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size[0] as i32,
                size[1] as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            tex.0.get()
        }
    }

    /// The same pixels as `texture`, but bound as `GL_TEXTURE_EXTERNAL_OES`
    /// through an EGLImage (how a dmabuf arrives). `None` without
    /// `EGL_KHR_gl_texture_2D_image` / `GL_OES_EGL_image_external_essl3`.
    pub fn external_texture(&self, rgba: &[u8], size: [u32; 2]) -> Option<u32> {
        let e = &egl::API;
        let egl_exts = e
            .query_string(Some(self.display), egl::EXTENSIONS)
            .ok()?
            .to_string_lossy()
            .into_owned();
        if !egl_exts.contains("EGL_KHR_gl_texture_2D_image")
            || !self.has_ext("GL_OES_EGL_image_external_essl3")
        {
            return None;
        }
        let src = self.texture(rgba, size);
        let image = e
            .create_image(
                self.display,
                unsafe { egl::Context::from_ptr(self.context.as_ptr()) },
                GL_TEXTURE_2D_KHR,
                unsafe { egl::ClientBuffer::from_ptr(src as usize as *mut c_void) },
                &[egl::ATTRIB_NONE],
            )
            .ok()?;
        type TargetFn = unsafe extern "system" fn(u32, *mut c_void);
        let f = e.get_proc_address("glEGLImageTargetTexture2DOES")?;
        let target: TargetFn = unsafe { std::mem::transmute(f) };
        unsafe {
            let gl = &self.gl;
            let tex = gl.create_texture().unwrap();
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(tex));
            target(TEXTURE_EXTERNAL_OES, image.as_ptr());
            gl.tex_parameter_i32(
                TEXTURE_EXTERNAL_OES,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                TEXTURE_EXTERNAL_OES,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
            let err = gl.get_error();
            (err == glow::NO_ERROR).then_some(tex.0.get())
        }
    }
}

pub const TEXTURE_EXTERNAL_OES: u32 = 0x8D65;

pub struct Fbo {
    pub fbo: glow::Framebuffer,
    pub rb: glow::Renderbuffer,
    pub size: [u32; 2],
}

impl Fbo {
    pub fn id(&self) -> u32 {
        self.fbo.0.get()
    }
}

pub fn flip_rows(px: &[u8], w: usize) -> Vec<u8> {
    px.chunks(w * 4).rev().flatten().copied().collect()
}

pub fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

pub fn write_png(path: &std::path::Path, rgba: &[u8], size: [u32; 2]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

pub fn read_png(path: &std::path::Path) -> Option<(Vec<u8>, [u32; 2])> {
    let dec = png::Decoder::new(std::fs::File::open(path).ok()?);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "{path:?} must be RGBA8"
    );
    buf.truncate(info.buffer_size());
    Some((buf, [info.width, info.height]))
}

/// Per-channel difference > 8/255 on at most 0.5 % of the pixels.
pub fn compare(name: &str, got: &[u8], size: [u32; 2]) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        write_png(&path, got, size);
        eprintln!("updated {path:?}");
        return;
    }
    let (want, wsize) = read_png(&path)
        .unwrap_or_else(|| panic!("missing golden {path:?}; run with UPDATE_GOLDEN=1"));
    assert_eq!(wsize, size, "{name}: golden size");
    let mut bad = 0usize;
    let mut worst = 0u8;
    for (a, b) in got.chunks(4).zip(want.chunks(4)) {
        let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        worst = worst.max(d);
        if d > 8 {
            bad += 1;
        }
    }
    let total = (size[0] * size[1]) as usize;
    if bad * 200 > total {
        let out = std::env::temp_dir().join(format!("nkw-gl-{name}-actual.png"));
        write_png(&out, got, size);
        panic!(
            "{name}: {bad}/{total} px differ by > 8/255 (worst {worst}); actual written to {out:?}"
        );
    }
}

#[test]
fn surfaceless_context_comes_up() {
    let Some(ctx) = context() else { return };
    let v = ctx.gl.version();
    assert!(v.is_embedded && v.major >= 3, "{v:?}");
    eprintln!("GL renderer: {}", ctx.renderer());
}
