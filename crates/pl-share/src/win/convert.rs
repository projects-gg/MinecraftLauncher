//! BGRA → NV12 with scaling on the GPU (D3D11 video processor), one per layer. The output size is
//! fixed for the encoder's life; a source that changes size is letterboxed into it (black bars,
//! aspect kept), so a window being resized never restarts the encoder.

use std::mem::ManuallyDrop;
use std::sync::Arc;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_VPIV,
    D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_COLOR,
    D3D11_VIDEO_COLOR_0, D3D11_VIDEO_COLOR_YCbCrA, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_OPTIMAL_SPEED, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoContext1,
    ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
    ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::core::BOOL;
use windows::core::{Interface, Result};

use super::d3d::Gpu;

/// Encoders keep a few input frames in flight; the ring is larger than any MFT's queue seen.
const RING: usize = 6;

pub struct Scaler {
    gpu: Arc<Gpu>,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    /// Rebuilt when the source size changes.
    processor: Option<(
        ID3D11VideoProcessorEnumerator,
        ID3D11VideoProcessor,
        (u32, u32),
    )>,
    out: (u32, u32),
    ring: Vec<(ID3D11Texture2D, Option<ID3D11VideoProcessorOutputView>)>,
    next: usize,
    /// SQOL: a small black picture, made the first time a blank frame is needed.
    black: Option<ID3D11Texture2D>,
}

// SAFETY: D3D11 video objects are free-threaded; the immediate context is serialised by
// `Gpu::lock` around every use, and a scaler is used by one thread at a time (behind a Mutex).
unsafe impl Send for Scaler {}

impl Scaler {
    pub fn new(gpu: Arc<Gpu>, out: (u32, u32)) -> Result<Self> {
        let video_device: ID3D11VideoDevice = gpu.device.cast()?;
        let video_context: ID3D11VideoContext = gpu.context.cast()?;
        let mut ring = Vec::with_capacity(RING);
        for _ in 0..RING {
            ring.push((nv12_texture(&gpu, out)?, None));
        }
        Ok(Self {
            gpu,
            video_device,
            video_context,
            processor: None,
            out,
            ring,
            next: 0,
            black: None,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        self.out
    }

    /// SQOL: a black frame at the output size (a switch, a hold, a closed source): through the
    /// same conversion as a captured frame, so every encoder takes it as it takes those.
    pub fn blank(&mut self) -> Result<(ID3D11Texture2D, RECT)> {
        if self.black.is_none() {
            self.black = Some(black_texture(&self.gpu)?);
        }
        let black = self.black.clone().ok_or_else(windows::core::Error::empty)?;
        self.convert(&black, (BLACK_SIDE, BLACK_SIDE))
    }

    fn processor(
        &mut self,
        src: (u32, u32),
    ) -> Result<(ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor)> {
        if let Some((e, p, size)) = &self.processor
            && *size == src
        {
            return Ok((e.clone(), p.clone()));
        }
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            InputWidth: src.0,
            InputHeight: src.1,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            OutputWidth: self.out.0,
            OutputHeight: self.out.1,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        // SAFETY: live video device; descriptors outlive the calls.
        let (enumerator, processor) = unsafe {
            let enumerator = self
                .video_device
                .CreateVideoProcessorEnumerator(&desc)
                .map_err(at("enumerator"))?;
            let processor = self
                .video_device
                .CreateVideoProcessor(&enumerator, 0)
                .map_err(at("processor"))?;
            (enumerator, processor)
        };
        // SAFETY: the processor was just made on this context's device.
        unsafe {
            let ctx = &self.video_context;
            // Studio-range BT.709 out of full-range sRGB in: what every H.264 decoder assumes.
            if let Ok(ctx1) = ctx.cast::<ID3D11VideoContext1>() {
                ctx1.VideoProcessorSetStreamColorSpace1(
                    &processor,
                    0,
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                );
                ctx1.VideoProcessorSetOutputColorSpace1(
                    &processor,
                    DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
                );
            }
            let black = D3D11_VIDEO_COLOR {
                Anonymous: D3D11_VIDEO_COLOR_0 {
                    YCbCr: D3D11_VIDEO_COLOR_YCbCrA {
                        Y: 0.0625,
                        Cb: 0.5,
                        Cr: 0.5,
                        A: 1.0,
                    },
                },
            };
            ctx.VideoProcessorSetOutputBackgroundColor(&processor, true, &black);
            // No auto-processing (denoise, edge enhancement) on text.
            ctx.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
        }
        // The output views belong to the enumerator: rebuild them.
        for slot in &mut self.ring {
            slot.1 = None;
        }
        self.processor = Some((enumerator.clone(), processor.clone(), src));
        Ok((enumerator, processor))
    }

    /// Converts the `content` part of `src` (a capture texture, possibly larger than its content)
    /// into the next ring texture, letterboxed. Returns that texture and where the picture sits in
    /// it (the still picture of SHARE-LIVE is cut from there).
    pub fn convert(
        &mut self,
        src: &ID3D11Texture2D,
        content: (u32, u32),
    ) -> Result<(ID3D11Texture2D, RECT)> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a live texture.
        unsafe { src.GetDesc(&mut desc) };
        let src_size = (desc.Width, desc.Height);
        let content = (
            content.0.min(desc.Width).max(2),
            content.1.min(desc.Height).max(2),
        );
        let (enumerator, processor) = self.processor(src_size)?;
        let slot = self.next;
        self.next = (self.next + 1) % self.ring.len();
        let target = self.ring[slot].0.clone();
        if self.ring[slot].1.is_none() {
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut view = None;
            // SAFETY: the texture and enumerator are live.
            unsafe {
                self.video_device
                    .CreateVideoProcessorOutputView(
                        &target,
                        &enumerator,
                        &view_desc,
                        Some(&mut view),
                    )
                    .map_err(at("output view"))?;
            }
            self.ring[slot].1 = view;
        }
        let output_view = self.ring[slot]
            .1
            .clone()
            .ok_or_else(windows::core::Error::empty)?;

        let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view: Option<ID3D11VideoProcessorInputView> = None;
        // SAFETY: live objects.
        unsafe {
            self.video_device
                .CreateVideoProcessorInputView(src, &enumerator, &input_desc, Some(&mut input_view))
                .map_err(at("input view"))?;
        }

        let source_rect = RECT {
            left: 0,
            top: 0,
            right: content.0 as i32,
            bottom: content.1 as i32,
        };
        let dest = letterbox(content, self.out);
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: BOOL(1),
            pInputSurface: ManuallyDrop::new(input_view),
            ..Default::default()
        };
        let _lock = self.gpu.lock();
        // SAFETY: all views belong to `processor`'s enumerator; `stream` is released below.
        let result = unsafe {
            let ctx = &self.video_context;
            ctx.VideoProcessorSetStreamSourceRect(&processor, 0, true, Some(&source_rect));
            ctx.VideoProcessorSetStreamDestRect(&processor, 0, true, Some(&dest));
            ctx.VideoProcessorSetOutputTargetRect(&processor, false, None);
            ctx.VideoProcessorBlt(&processor, &output_view, 0, std::slice::from_ref(&stream))
        };
        // SAFETY: taken once; the view's reference is released here.
        unsafe { ManuallyDrop::drop(&mut stream.pInputSurface) };
        result.map_err(at("blt"))?;
        Ok((target, dest))
    }
}

/// Names the failing step in the error (logged by the session).
fn at(step: &'static str) -> impl FnOnce(windows::core::Error) -> windows::core::Error {
    move |e| windows::core::Error::new(e.code(), format!("{step}: {}", e.message()))
}

/// The largest rectangle with `content`'s aspect ratio centred in `out`, even-aligned.
fn letterbox(content: (u32, u32), out: (u32, u32)) -> RECT {
    let scale = (out.0 as f64 / content.0 as f64).min(out.1 as f64 / content.1 as f64);
    let w = ((content.0 as f64 * scale).round() as i32).min(out.0 as i32) & !1;
    let h = ((content.1 as f64 * scale).round() as i32).min(out.1 as i32) & !1;
    let left = ((out.0 as i32 - w) / 2) & !1;
    let top = ((out.1 as i32 - h) / 2) & !1;
    RECT {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

/// The black picture's side (letterboxed into any output with black bars: all of it is black;
/// video processors take inputs this small, as the encoders' smallest picture).
const BLACK_SIDE: u32 = 64;

/// An opaque black BGRA texture, the kind a capture delivers.
fn black_texture(gpu: &Gpu) -> Result<ID3D11Texture2D> {
    let side = BLACK_SIDE as usize;
    let mut pixels = vec![0u8; side * side * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 0xFF;
    }
    let desc = D3D11_TEXTURE2D_DESC {
        Width: BLACK_SIDE,
        Height: BLACK_SIDE,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: (side * 4) as u32,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    // SAFETY: valid descriptor; `pixels` outlives the call and holds a full mip level.
    unsafe {
        gpu.device
            .CreateTexture2D(&desc, Some(&data), Some(&mut texture))
    }
    .map_err(at("black texture"))?;
    texture.ok_or_else(windows::core::Error::empty)
}

fn nv12_texture(gpu: &Gpu, size: (u32, u32)) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: size.0,
        Height: size.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        // What Chromium gives its MF encoder input: render target (video processor output) and
        // shader resource.
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: valid descriptor; out-parameter valid.
    unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(at("nv12 texture"))?;
    texture.ok_or_else(windows::core::Error::empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SQOL, on this computer's GPU (`cargo test -p pl-share -- --ignored`): the blank frame is
    /// studio-range black everywhere (luma 16, no colour), at the scaler's size.
    #[test]
    #[ignore = "needs a GPU with a video processor"]
    fn a_blank_frame_is_black() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_USAGE_STAGING,
        };
        super::super::com_mta();
        let gpu = Arc::new(Gpu::new().expect("a GPU"));
        let mut scaler = Scaler::new(gpu.clone(), (640, 360)).unwrap();
        let (texture, _) = scaler.blank().unwrap();
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 640,
            Height: 360,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging = None;
        // SAFETY: valid descriptor; the copy and the map are on live textures of one size.
        unsafe {
            gpu.device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .unwrap();
            let staging = staging.unwrap();
            let _lock = gpu.lock();
            gpu.context.CopyResource(&staging, &texture);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map))
                .unwrap();
            let pitch = map.RowPitch as usize;
            let rows = std::slice::from_raw_parts(map.pData as *const u8, pitch * 360 * 3 / 2);
            for (y, x) in [(0, 0), (180, 320), (359, 639)] {
                assert!(rows[y * pitch + x].abs_diff(16) <= 2, "luma at {x},{y}");
            }
            let chroma = &rows[pitch * 360..];
            assert!(chroma[pitch * 90 + 320].abs_diff(128) <= 2);
            gpu.context.Unmap(&staging, 0);
        }
    }

    #[test]
    fn letterbox_centres_and_keeps_aspect() {
        let r = letterbox((1280, 720), (1280, 720));
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 0, 1280, 720));
        // A 4:3 source in a 16:9 frame: pillarbox.
        let r = letterbox((800, 600), (1280, 720));
        assert_eq!((r.right - r.left, r.bottom - r.top), (960, 720));
        assert_eq!(r.left, 160);
        // A wide source: letterbox.
        let r = letterbox((2560, 720), (1280, 720));
        assert_eq!((r.right - r.left, r.bottom - r.top), (1280, 360));
        assert_eq!(r.top, 180);
    }
}
