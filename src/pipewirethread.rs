use libwayshot::CaptureRegion;
use pipewire::{
    spa::{
        self,
        pod::{self, serialize::PodSerializer},
        utils::Id,
    },
    stream::StreamState,
};
use std::{
    io,
    os::fd::{BorrowedFd, IntoRawFd},
    slice,
};
use tokio::sync::oneshot;
use wayland_client::protocol::wl_output;
pub async fn start_cast(
    overlay_cursor: bool,
    width: u32,
    height: u32,
    capture_region: Option<CaptureRegion>,
    output: wl_output::WlOutput,
) -> anyhow::Result<u32> {
    let (tx, rx) = oneshot::channel();
    //let (_, _thread_stop_rx) = pipewire::channel::channel::<()>();
    std::thread::spawn(move || {
        let _ = start_stream(tx, overlay_cursor, width, height, capture_region, output);
    });
    Ok(rx.await?)
}

fn start_stream(
    sender: oneshot::Sender<u32>,
    _overlay_cursor: bool,
    width: u32,
    height: u32,
    capture_region: Option<CaptureRegion>,
    output: wl_output::WlOutput,
) -> Result<(), pipewire::Error> {
    let connection = libwayshot::WayshotConnection::new().unwrap();
    let loop_ = pipewire::MainLoop::new()?;
    let context = pipewire::Context::new(&loop_).unwrap();
    let core = context.connect(None).unwrap();

    let name = "wayshot-screenshot"; // XXX randomize?

    let stream = pipewire::stream::Stream::new(
        &core,
        name,
        //pipewire::properties! {
        //    *pipewire::keys::MEDIA_TYPE => "Video",
        //    *pipewire::keys::MEDIA_CATEGORY => "Capture",
        //    *pipewire::keys::MEDIA_ROLE => "Screen",
        //},
        pipewire::properties! {
            "media.class" => "Video/Source",
            "node.name" => "wayshot-screenshot", // XXX
        },
    )?;

    //let stream_cell: Rc<RefCell<Option<pipewire::stream::Stream>>> = Rc::new(RefCell::new(None));
    //let stream_cell_clone = stream_cell.clone();

    //let (node_id_tx, node_id_rx) = oneshot::channel();
    //let mut node_id_tx = Some(node_id_tx);
    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |old, new| {
            println!("state-changed '{:?}' -> '{:?}'", old, new);
            match new {
                StreamState::Streaming => {
                    println!("Streaming");
                }
                StreamState::Paused => {
                    println!("ssseeee");
                    //let stream = stream_cell_clone.borrow_mut();
                    //let stream = stream.as_ref().unwrap();
                    //if let Some(node_id_tx) = node_id_tx.take() {
                    //    node_id_tx.send(Ok(stream.node_id())).unwrap();
                    //}
                    //println!("{}", stream.node_id());
                }
                StreamState::Error(_) => {
                    println!("Errror");
                    //if let Some(node_id_tx) = node_id_tx.take() {
                    //    node_id_tx
                    //        .send(Err(anyhow::anyhow!("stream error: {}", msg)))
                    //        .unwrap();
                    //}
                }
                _ => {}
            }
        })
        .param_changed(|_, id, (), pod| {
            println!("ssssssssssssssssssssssssssssssss");
            if id != libspa_sys::SPA_PARAM_Format {
                return;
            }
            if let Some(pod) = pod {
                println!("param-changed: {} {:?}", id, pod.size());
            }
        })
        .add_buffer(move |buffer| {
            println!("ssseeeeeeee");
            let buf = unsafe { &mut *(*buffer).buffer };
            let datas = unsafe { slice::from_raw_parts_mut(buf.datas, buf.n_datas as usize) };
            for data in datas {
                use std::ffi::CStr;
                let name = unsafe { CStr::from_bytes_with_nul_unchecked(b"pipewire-screencopy\0") };
                let fd = rustix::fs::memfd_create(name, rustix::fs::MemfdFlags::CLOEXEC).unwrap();
                rustix::fs::ftruncate(&fd, (width * height * 4) as _).unwrap();

                data.type_ = libspa_sys::SPA_DATA_MemFd;
                data.flags = 0;
                data.fd = fd.into_raw_fd().into();

                data.data = std::ptr::null_mut();
                data.maxsize = width * height * 4;
                data.mapoffset = 0;
                let chunk = unsafe { &mut *data.chunk };
                chunk.size = width * height * 4;
                chunk.offset = 0;
                chunk.stride = 4 * width as i32;
            }
        })
        .remove_buffer(|buffer| {
            let buf = unsafe { &mut *(*buffer).buffer };
            let datas = unsafe { slice::from_raw_parts_mut(buf.datas, buf.n_datas as usize) };

            for data in datas {
                let _ = unsafe { rustix::io::close(data.fd as _) };
                data.fd = -1;
            }
        })
        .process(move |stream, ()| {
            if let Some(mut buffer) = stream.dequeue_buffer() {
                println!("sssbb");
                let datas = buffer.datas_mut();
                //let data = datas[0].get_mut();
                //if data.len() == width as usize * height as usize * 4 {
                let fd = unsafe { BorrowedFd::borrow_raw(datas[0].as_raw().fd as _) };
                // TODO error
                connection
                    .capture_output_frame_shm_fd(
                        0,
                        output.clone(),
                        libwayshot::reexport::Transform::Normal,
                        fd,
                        capture_region,
                    )
                    .unwrap();
                println!("beta");
            }
        })
        .register()?;
    let format = format(width, height);
    let buffers = buffers(width as u32, height as u32);
    let params = &mut [
        pod::Pod::from_bytes(&format).unwrap(),
        pod::Pod::from_bytes(&buffers).unwrap(),
    ];
    //let flags = pipewire::stream::StreamFlags::MAP_BUFFERS;
    let flags =
        pipewire::stream::StreamFlags::ALLOC_BUFFERS | pipewire::stream::StreamFlags::DRIVER;
    stream.connect(spa::Direction::Output, None, flags, params)?;

    sender.send(stream.node_id()).unwrap();
    loop_.run();
    Ok(())
    //Ok((loop_, node_id))
}

fn value_to_bytes(value: pod::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut cursor = io::Cursor::new(&mut bytes);
    PodSerializer::serialize(&mut cursor, &value).unwrap();
    bytes
}

fn buffers(width: u32, height: u32) -> Vec<u8> {
    value_to_bytes(pod::Value::Object(pod::Object {
        type_: libspa_sys::SPA_TYPE_OBJECT_ParamBuffers,
        id: libspa_sys::SPA_PARAM_Buffers,
        properties: vec![
            /*
            pod::Property {
                key: spa_sys::SPA_PARAM_BUFFERS_dataType,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Choice(pod::ChoiceValue::Int(spa::utils::Choice(
                    spa::utils::ChoiceFlags::empty(),
                    spa::utils::ChoiceEnum::Flags {
                        default: 1 << spa_sys::SPA_DATA_MemFd,
                        flags: vec![],
                    },
                ))),
            },
            */
            pod::Property {
                key: libspa_sys::SPA_PARAM_BUFFERS_size,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Int(width as i32 * height as i32 * 4),
            },
            pod::Property {
                key: libspa_sys::SPA_PARAM_BUFFERS_stride,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Int(width as i32 * 4),
            },
            pod::Property {
                key: libspa_sys::SPA_PARAM_BUFFERS_align,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Int(16),
            },
            pod::Property {
                key: libspa_sys::SPA_PARAM_BUFFERS_blocks,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Int(1),
            },
            pod::Property {
                key: libspa_sys::SPA_PARAM_BUFFERS_buffers,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Choice(pod::ChoiceValue::Int(spa::utils::Choice(
                    spa::utils::ChoiceFlags::empty(),
                    spa::utils::ChoiceEnum::Range {
                        default: 4,
                        min: 1,
                        max: 32,
                    },
                ))),
            },
        ],
    }))
}

fn format(width: u32, height: u32) -> Vec<u8> {
    value_to_bytes(pod::Value::Object(pod::Object {
        type_: libspa_sys::SPA_TYPE_OBJECT_Format,
        id: libspa_sys::SPA_PARAM_EnumFormat,
        properties: vec![
            pod::Property {
                key: libspa_sys::SPA_FORMAT_mediaType,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Id(Id(libspa_sys::SPA_MEDIA_TYPE_video)),
            },
            pod::Property {
                key: libspa_sys::SPA_FORMAT_mediaSubtype,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Id(Id(libspa_sys::SPA_MEDIA_SUBTYPE_raw)),
            },
            pod::Property {
                key: libspa_sys::SPA_FORMAT_VIDEO_format,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Id(Id(libspa_sys::SPA_VIDEO_FORMAT_RGBA)),
            },
            // XXX modifiers
            pod::Property {
                key: libspa_sys::SPA_FORMAT_VIDEO_size,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Rectangle(spa::utils::Rectangle { width, height }),
            },
            pod::Property {
                key: libspa_sys::SPA_FORMAT_VIDEO_framerate,
                flags: pod::PropertyFlags::empty(),
                value: pod::Value::Fraction(spa::utils::Fraction { num: 60, denom: 1 }),
            },
            // TODO max framerate
        ],
    }))
}
