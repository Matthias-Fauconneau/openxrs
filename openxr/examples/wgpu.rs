use openxr as xr;
fn main() -> xr::Result<()> {
    std::env::set_var("XR_RUNTIME_JSON", "C:\\Users\\spadmin\\Documents\\GitHub\\openxrs\\target\\debug\\examples\\RemotingXR.json");
    let xr = xr::Entry::linked();
    //let xr = unsafe{xr::Entry::load().unwrap()};
    let available_extensions = xr.enumerate_extensions()?;
    assert!(available_extensions.khr_d3d12_enable);
    assert!(available_extensions.msft_holographic_remoting);
    let mut enabled_extensions = xr::ExtensionSet::default();
    enabled_extensions.khr_d3d12_enable = true;
    enabled_extensions.msft_holographic_remoting = true;
    let xr = xr.create_instance(&xr::ApplicationInfo{application_name: "IR", engine_name: "Rust", ..Default::default()}, &enabled_extensions, &[])?;
    dbg!(xr.properties()?);
    let system = xr.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;
    dbg!();
    xr::cvt(unsafe{(xr.exts().msft_holographic_remoting.unwrap().remoting_connect)(xr.as_raw(), system, &xr::sys::RemotingConnectInfoMSFT {
            ty: xr::StructureType::REMOTING_CONNECT_INFO_MSFT,
            next: std::ptr::null(),
            remote_host_name: std::ffi::CStr::from_bytes_with_nul(b"10.6.188.27\0").unwrap().as_ptr(),
            remote_port: 8265,
            secure_connection: false.into(),
    })}).unwrap();
    dbg!();
    let view_type = xr::ViewConfigurationType::PRIMARY_STEREO;
    let environment_blend_mode = xr.enumerate_environment_blend_modes(system, view_type)?[0];
    use pollster::FutureExt as _;
    let adapter = wgpu::Instance::new(wgpu::Backend::Dx12.into()).request_adapter(&Default::default()).block_on().unwrap();
    let (device, queue) = adapter.request_device(&Default::default(), None).block_on().unwrap();
    use wgpu_hal::api::Dx12;
    let (session, mut frame_wait, mut frame_stream) = unsafe {
        let (device, queue) = device.as_hal::<Dx12, _, _>(|device| (device.unwrap().raw_device().as_ptr(), device.unwrap().raw_queue().as_ptr()));
        xr.create_session::<xr::D3D12>(system, &xr::d3d12::SessionCreateInfo{device: device.cast(), queue: queue.cast()})
    }?;
    let vert_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor{label: None, source: wgpu::util::make_spirv(include_bytes!("fullscreen.vert.spv"))});
    let frag_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor{label: None, source: wgpu::util::make_spirv(include_bytes!("debug_pattern.frag.spv"))});
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label: None, bind_group_layouts: &[], push_constant_ranges: &[]});
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    const VIEW_COUNT: u32 = 2;
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState{module: &vert_shader, entry_point: "main", buffers: &[]},
        fragment: Some(wgpu::FragmentState{module: &frag_shader, entry_point: "main", targets: &[Some(format.into())]}),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: VIEW_COUNT.try_into().ok(),
    });
    let views = xr.enumerate_view_configuration_views(system, view_type)?;
    assert_eq!(views.len(), VIEW_COUNT as usize); assert_eq!(views[0], views[1]);
    let xr::ViewConfigurationView{recommended_image_rect_width: width, recommended_image_rect_height: height, ..} = views[0];
    let mut swapchain = session.create_swapchain(&xr::SwapchainCreateInfo{
        create_flags: xr::SwapchainCreateFlags::EMPTY,
        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::SAMPLED,
        format: winapi::shared::dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_count: 1,
        width, height,
        face_count: 1,
        array_size: VIEW_COUNT,
        mip_count: 1,
    })?;
    let images = swapchain.enumerate_images()?.into_iter().map(|image| {
        let desc = wgpu::TextureDescriptor {label: None, size: wgpu::Extent3d{width, height, depth_or_array_layers: VIEW_COUNT}, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::TEXTURE_BINDING};
        unsafe{device.create_texture_from_hal::<Dx12>(<Dx12 as wgpu_hal::Api>::Device::texture_from_raw(d3d12::Resource::from_raw(image.cast()), desc.format, desc.dimension, desc.size, desc.mip_level_count, desc.sample_count), &desc)}
    }).collect::<Box<_>>();
    let stage = session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)?;
    loop {
        let mut session_running = false;
        let mut event_storage = xr::EventDataBuffer::new();
        while let Some(event) = xr.poll_event(&mut event_storage)? {
            use xr::Event::*; match event {
                SessionStateChanged(e) => {
                    println!("entered state {:?}", e.state());
                    match e.state() {
                        xr::SessionState::READY => { session.begin(view_type)?; session_running = true; }
                        xr::SessionState::STOPPING => { session.end()?; session_running = false; }
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => { return Ok(()); }
                        _ => {dbg!()}
                    }
                }
                InstanceLossPending(_) => { return Ok(()); }
                EventsLost(e) => { println!("lost {} events", e.lost_event_count()); }
                _ => {dbg!()}
            }
        }
        assert!(session_running);
        let frame_state = frame_wait.wait()?;
        frame_stream.begin()?;
        if !frame_state.should_render { frame_stream.end(frame_state.predicted_display_time, environment_blend_mode, &[])?; continue; }
        let index = swapchain.acquire_image()? as usize;
        swapchain.wait_image(xr::Duration::INFINITE)?;
        let ref view = images[index].create_view(&wgpu::TextureViewDescriptor{base_array_layer: 0, array_layer_count: VIEW_COUNT.try_into().ok(), ..Default::default()});
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor{label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment{view, resolve_target: None, ops: wgpu::Operations{load: wgpu::LoadOp::Clear(wgpu::Color::GREEN), store: true}})], depth_stencil_attachment: None});
            pass.set_pipeline(&render_pipeline);
            pass.draw(0..3, 0..1);}
        let (_, views) = session.locate_views(view_type, frame_state.predicted_display_time, &stage)?;
        queue.submit(Some(encoder.finish()));
        swapchain.release_image()?;
        let rect = xr::Rect2Di {offset: xr::Offset2Di{x: 0, y: 0}, extent: xr::Extent2Di{width: width as i32, height: height as i32}};
        frame_stream.end(frame_state.predicted_display_time, environment_blend_mode, &[&xr::CompositionLayerProjection::new().space(&stage).views(&[0,1].map(|i|
            xr::CompositionLayerProjectionView::new().pose(views[i].pose).fov(views[i].fov).sub_image(xr::SwapchainSubImage::new().swapchain(&swapchain).image_array_index(i as u32).image_rect(rect))))])?;
    }
}