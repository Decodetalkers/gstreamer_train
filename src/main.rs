use ashpd::{
    desktop::screencast::{CursorMode, PersistMode, Screencast, SourceType},
    WindowIdentifier,
};
//use gstreamer::{prelude::ObjectExt, traits::ElementExt, MessageType};
use gstreamer::{prelude::{GstBinExtManual, ElementExtManual}, traits::ElementExt, MessageType};
//use gstreamer::{traits::ElementExt, MessageType, prelude::GstBinExtManual};
use std::os::unix::io::AsRawFd;

fn screen_gstreamer<F: AsRawFd>(fd: F, node_id: Option<u32>) -> anyhow::Result<()> {
    gstreamer::init()?;
    let raw_fd = fd.as_raw_fd();
    let element = gstreamer::Pipeline::new(None);
    let videoconvert = gstreamer::ElementFactory::make("videoconvert").build()?;
    let ximagesink = gstreamer::ElementFactory::make("ximagesink").build()?;
    if let Some(node) = node_id {
        let pipewire_element = gstreamer::ElementFactory::make("pipewiresrc")
            .property("fd", &raw_fd)
            .property("path", &node.to_string())
            .build()?;
        element.add_many(&[&pipewire_element, &videoconvert, &ximagesink])?; 
        pipewire_element.link(&videoconvert)?;
        videoconvert.link(&ximagesink)?;
        element.set_state(gstreamer::State::Playing)?;
        let bus = element.bus();
        let message = bus
            .unwrap()
            .timed_pop_filtered(None, &[MessageType::Error, MessageType::Eos]);
        dbg!(message);
        element.set_state(gstreamer::State::Null)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let proxy = Screencast::new().await?;
    let session = proxy.create_session().await?;
    proxy
        .select_sources(
            &session,
            CursorMode::Hidden,
            SourceType::Monitor | SourceType::Window,
            true,
            None,
            PersistMode::DoNot,
        )
        .await?;

    let response = proxy
        .start(&session, &WindowIdentifier::default())
        .await?
        .response()?;
    let fd = proxy.open_pipe_wire_remote(&session).await?;
    response.streams().iter().for_each(|stream| {
        println!("node id: {}", stream.pipe_wire_node_id());
        println!("size: {:?}", stream.size());
        println!("position: {:?}", stream.position());
        screen_gstreamer(fd, Some(stream.pipe_wire_node_id())).unwrap();
    });
    Ok(())
}
