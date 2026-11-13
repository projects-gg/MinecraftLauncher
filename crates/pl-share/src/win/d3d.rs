//! One Direct3D 11 device for capture, conversion and encoding, so frames never leave the GPU.

use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::{HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::core::{Interface, Result};

pub struct Gpu {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    /// The same device for Windows.Graphics.Capture.
    pub winrt: IDirect3DDevice,
    multithread: ID3D11Multithread,
    pub luid: LUID,
    pub vendor_id: u32,
    pub adapter: String,
}

// SAFETY: the device is free-threaded and the immediate context is protected by
// `ID3D11Multithread::SetMultithreadProtected(true)`; multi-call sequences use `lock`.
unsafe impl Send for Gpu {}
// SAFETY: as above.
unsafe impl Sync for Gpu {}

impl Gpu {
    /// A hardware device on the default adapter (the one driving the primary display, which is also
    /// where Windows composes the windows we capture).
    pub fn new() -> Result<Self> {
        let mut device = None;
        let mut context = None;
        // SAFETY: out-parameters are valid for the call.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }
        let device = device.ok_or_else(windows::core::Error::empty)?;
        let context = context.ok_or_else(windows::core::Error::empty)?;
        let multithread: ID3D11Multithread = context.cast()?;
        // SAFETY: a live context.
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
        let dxgi: IDXGIDevice = device.cast()?;
        // SAFETY: live DXGI objects.
        let (luid, vendor_id, adapter) = unsafe {
            let desc = dxgi.GetAdapter()?.GetDesc()?;
            let len = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            (
                desc.AdapterLuid,
                desc.VendorId,
                String::from_utf16_lossy(&desc.Description[..len]),
            )
        };
        // SAFETY: a live DXGI device.
        let winrt: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        Ok(Self {
            device,
            context,
            winrt,
            multithread,
            luid,
            vendor_id,
            adapter,
        })
    }

    /// SQOL: the device was lost (a driver update or reset, the GPU removed): everything made on it
    /// is unusable and the share is rebuilt on a new device.
    pub fn removed(&self) -> bool {
        // SAFETY: a plain query on a live device.
        unsafe { self.device.GetDeviceRemovedReason() }.is_err()
    }

    /// Holds the immediate context for a sequence of calls.
    pub fn lock(&self) -> ContextLock<'_> {
        // SAFETY: balanced by Leave in Drop.
        unsafe { self.multithread.Enter() };
        ContextLock(self)
    }
}

pub struct ContextLock<'a>(&'a Gpu);

impl Drop for ContextLock<'_> {
    fn drop(&mut self) {
        // SAFETY: entered in `Gpu::lock`.
        unsafe { self.0.multithread.Leave() };
    }
}
