pub mod client;
use x11rb::protocol::render::ConnectionExt;
pub mod layout;
pub mod wm;
use crate::wm::*;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::render::{self, PictType};

use x11rb::protocol::xproto::*;

fn example_get_visual2<C: Connection>(
    conn: &C,
    windowvisual: usize,
    screen_num: usize,
) -> Visualtype {
    // Open the connection to the X server. Use the DISPLAY environment variable.
    let screen = &conn.setup().roots[screen_num];

    for depth in &screen.allowed_depths {
        for visualtype in &depth.visuals {
            println!("visualtype: {:#?}", visualtype);
            if visualtype.visual_id == windowvisual.try_into().unwrap() {
                return visualtype.clone();
            }
        }
    }
    return Visualtype {
        visual_id: 0,
        class: 0.into(),
        bits_per_rgb_value: 0,
        colormap_entries: 0,
        red_mask: 0,
        green_mask: 0,
        blue_mask: 0,
    };
}
fn choose_visual(conn: &impl Connection, screen_num: usize) -> Result<(u8, Visualid), ReplyError> {
    let depth = 32;
    let screen = &conn.setup().roots[screen_num];

    // Try to use XRender to find a visual with alpha support
    let has_render = conn
        .extension_information(render::X11_EXTENSION_NAME)?
        .is_some();
    if has_render {
        let formats = conn.render_query_pict_formats()?.reply()?;
        // Find the ARGB32 format that must be supported.
        let format = formats
            .formats
            .iter()
            .filter(|info| (info.type_, info.depth) == (PictType::DIRECT, depth))
            .filter(|info| {
                let d = info.direct;
                println!("d: {:#?}", d);
                (d.red_mask, d.green_mask, d.blue_mask, d.alpha_mask) == (0xff, 0xff, 0xff, 0xff)
            })
            .find(|info| {
                let d = info.direct;
                (d.red_shift, d.green_shift, d.blue_shift, d.alpha_shift) == (16, 8, 0, 24)
            });
        if let Some(format) = format {
            // Now we need to find the visual that corresponds to this format
            if let Some(visual) = formats.screens[screen_num]
                .depths
                .iter()
                .flat_map(|d| &d.visuals)
                .find(|v| v.format == format.id)
            {
                return Ok((format.depth, visual.visual));
            }
        }
    }
    Ok((screen.root_depth, screen.root_visual))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = WindowManager::new();
    manager.run()?;
    Ok(())
}
