//use gstreamer::{prelude::ObjectExt, traits::ElementExt, MessageType};
use gstreamer::{
    prelude::{ElementExtManual, GstBinExtManual},
    traits::ElementExt,
    MessageType,
};
use pipewirethread::start_cast;
mod pipewirethread;
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
        println!("sdfd");
        element.set_state(gstreamer::State::Playing)?;
        println!("sdfd2");
        let bus = element.bus().unwrap();
        loop {
            let message = bus.timed_pop_filtered(
                Some(gstreamer::ClockTime::from_useconds(1)),
                &[MessageType::Error, MessageType::Eos],
            );
            if let Some(message) = message {
                println!("Here is message");
                match message.type_() {
                    MessageType::Eos => {
                        println!("End");
                        break;
                    }
                    MessageType::Error => {
                        println!("{:?}", message);
                        println!("Error");
                        break;
                    }
                    _ => continue,
                }
            }
        }

        element.set_state(gstreamer::State::Null)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let connection = libwayshot::WayshotConnection::new().unwrap();
    let outputs = connection.get_all_outputs();
    let output = outputs[0].clone();
    let cast = start_cast(
        false,
        output.dimensions.width as u32,
        output.dimensions.height as u32,
        None,
        output.wl_output,
    )
    .await?;

    let fd = rustix::fs::memfd_create("helloworld", rustix::fs::MemfdFlags::CLOEXEC).unwrap();

    screen_gstreamer(fd, Some(cast)).unwrap();
    Ok(())
}
