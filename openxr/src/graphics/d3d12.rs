use std::ptr;

use sys::platform::*;

use crate::*;

/// The D3D12 graphics API
///
/// See [`XR_KHR_d3d12_enable`] for safety details.
///
/// [`XR_KHR_d3d12_enable`]: https://www.khronos.org/registry/OpenXR/specs/1.0/html/xrspec.html#XR_KHR_D3D12_enable
pub enum D3D12 {}

impl Graphics for D3D12 {
    type Requirements = Requirements;
    type SessionCreateInfo = SessionCreateInfo;
    type Format = u32;
    type SwapchainImage = *mut ID3D12Resource;

    fn raise_format(x: i64) -> u32 {
        x as _
    }
    fn lower_format(x: u32) -> i64 {
        x.into()
    }

    fn requirements(inst: &Instance, system: SystemId) -> Result<Requirements> {
        let out = unsafe {
            let mut x = sys::GraphicsRequirementsD3D12KHR::out(ptr::null_mut());
            cvt((inst.d3d12().get_d3d12_graphics_requirements)(
                inst.as_raw(),
                system,
                x.as_mut_ptr(),
            ))?;
            x.assume_init()
        };
        Ok(Requirements {
            adapter_luid: out.adapter_luid,
            min_feature_level: out.min_feature_level,
        })
    }

    unsafe fn create_session(
        instance: &Instance,
        system: SystemId,
        info: &Self::SessionCreateInfo,
    ) -> Result<sys::Session> {
        let binding = sys::GraphicsBindingD3D12KHR {
            ty: sys::GraphicsBindingD3D12KHR::TYPE,
            next: ptr::null(),
            device: info.device.cast_mut(),
            queue: info.queue.cast_mut(),
        };
        let info = sys::SessionCreateInfo {
            ty: sys::SessionCreateInfo::TYPE,
            next: &binding as *const _ as *const _,
            create_flags: Default::default(),
            system_id: system,
        };
        let mut out = sys::Session::NULL;
        cvt((instance.fp().create_session)(
            instance.as_raw(),
            &info,
            &mut out,
        ))?;
        Ok(out)
    }

    fn enumerate_swapchain_images(
        swapchain: &Swapchain<Self>,
    ) -> Result<Vec<Self::SwapchainImage>> {
        let images = get_arr_init(
            sys::SwapchainImageD3D12KHR {
                ty: sys::SwapchainImageD3D12KHR::TYPE,
                next: ptr::null_mut(),
                texture: ptr::null_mut(),
            },
            |capacity, count, buf| unsafe {
                (swapchain.instance().fp().enumerate_swapchain_images)(
                    swapchain.as_raw(),
                    capacity,
                    count,
                    buf as *mut _,
                )
            },
        )?;
        Ok(images.into_iter().map(|x| x.texture).collect())
    }
}

#[derive(Copy, Clone)]
pub struct Requirements {
    pub adapter_luid: LUID,
    pub min_feature_level: D3D_FEATURE_LEVEL,
}

#[derive(Copy, Clone)]
pub struct SessionCreateInfo {
    pub device: *const ID3D12Device,
    pub queue: *const ID3D12CommandQueue,
}
