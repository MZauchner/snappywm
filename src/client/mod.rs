use std::rc::Rc;

use crate::layout::Geom;
use x11rb::connection::Connection;
use x11rb::protocol::render::{self, ConnectionExt as _, PictType};
use x11rb::protocol::xproto::*;

use x11rb::rust_connection::*;
use x11rb::wrapper::ConnectionExt as Con;

//choose 32 bit depth visual
fn choose_visual(
    conn: &Rc<impl Connection>,
    screen_num: usize,
) -> Result<(u8, Visualid), ReplyError> {
    let depth = 32;
    let screen = &conn.setup().roots[screen_num];

    // Try to use XRender to find a visual with alpha support
    let has_render = conn
        .extension_information(x11rb::protocol::render::X11_EXTENSION_NAME)?
        .is_some();
    if has_render {
        let formats = conn.render_query_pict_formats()?.reply()?;
        // Find the ARGB32 format that must be supported.
        let format = formats
            .formats
            .iter()
            .filter(|info| {
                (info.type_, info.depth) == (x11rb::protocol::render::PictType::DIRECT, depth)
            })
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
#[derive(Debug, Clone)]
pub struct Client {
    conn: Option<Rc<RustConnection>>,
    pub frameid: Option<u32>,
    pub windowid: u32,
    x: u32,
    y: u32,
    colormap: u32,
    frameheight: u32,
    framewidth: u32,
    screen: Option<Screen>,
    floating: bool,
    //the fields below are for future features
    _monitor: u32,
    _focus: bool,
    _isfullscreen: bool,
    _tags: u32,
    pub mapped: bool, //windows are only framed when mapped
}

impl Default for Client {
    fn default() -> Client {
        Client {
            conn: None,
            frameid: None,
            windowid: 0,
            colormap: 0,
            //frame geometry initialized to zero before window gets mapped
            //could use option but it's easier to get values out this way
            x: 0,
            y: 0,
            frameheight: 0,
            framewidth: 0,
            screen: None,
            _monitor: 0,
            floating: false,
            _focus: false,
            _isfullscreen: false,
            _tags: 1,
            mapped: false,
        }
    }
}
impl Client {
    pub fn new(window: Window, screen: Screen, conn: Rc<RustConnection>) -> Box<Client> {
        let mut client = Box::from(Client::default());
        client.windowid = window;
        client.conn = Some(conn);
        client.screen = Some(screen);
        client
    }
    pub fn frame(&mut self, created_before_wm: bool) -> bool {
        let frameid = self
            .conn
            .as_ref()
            .expect("Client has no valid X connection")
            .generate_id()
            .unwrap();

        let window_attrs = self
            .conn
            .as_ref()
            .expect("Client has no valid X Connction")
            .get_window_attributes(self.windowid)
            .unwrap()
            .reply()
            .unwrap();

        //created before wm -- we basically only frame windows that were present before the launch
        //of the WM if they are mapped and if they did not set override_redirect
        if created_before_wm
            && (window_attrs.override_redirect || window_attrs.map_state != MapState::VIEWABLE)
        {
            return false;
        }
        //get window parent -- error panic if screen is not set: -- this should be the root window
        let parent = self.screen.clone().expect("Client screen not set");
        //get window geometry - we'll set the geometry to the current geometry

        let windowgeom = self
            .conn
            .as_ref()
            .expect("Client has no valid X connection")
            .get_geometry(self.windowid)
            .unwrap()
            .reply()
            .unwrap();

        let (mydepth, myvisual) = choose_visual(self.conn.as_ref().unwrap(), 0).unwrap();
        println!("depth: {} {}", windowgeom.depth, mydepth);
        println!("visual: {} {}", window_attrs.visual, myvisual);
        let atom = self
            .conn
            .as_ref()
            .unwrap()
            .intern_atom(false, b"_NET_WM_WINDOW_OPACITY")
            .unwrap()
            .reply()
            .unwrap();
        println!("atom: {:?}", atom);
        self.conn
            .as_ref()
            .unwrap()
            .change_property32(
                PropMode::REPLACE,
                self.windowid,
                atom.atom,
                AtomEnum::CARDINAL,
                &[0xffffffff as u32],
            )
            .unwrap()
            .check()
            .unwrap();

        //get correct depth
        //
        /*
        let mut d = 0 as u8;
        for depth in self.screen.clone().unwrap().allowed_depths {
            for visual in depth.visuals {
                if visual.visual_id == (window_attrs.visual) {
                    d = depth.depth;
                    println!("depth: {}", d);
                }
            }
        }
        */

        self.colormap = self.conn.as_ref().unwrap().generate_id().unwrap();
        if windowgeom.depth == 24 {
            self.conn
                .as_ref()
                .unwrap()
                .create_colormap(
                    ColormapAlloc::NONE,
                    self.colormap,
                    self.windowid,
                    window_attrs.visual,
                )
                .unwrap()
                .check()
                .expect("FAILED to create colormap");
        } else {
            self.conn
                .as_ref()
                .unwrap()
                .create_colormap(ColormapAlloc::NONE, self.colormap, self.windowid, myvisual)
                .unwrap()
                .check()
                .expect("FAILED to create colormap");
        }
        let winaux = CreateWindowAux::new()
            .event_mask(
                EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::SUBSTRUCTURE_REDIRECT
                    | EventMask::PROPERTY_CHANGE
                    | EventMask::BUTTON_PRESS
                    | EventMask::EXPOSURE
                    | EventMask::ENTER_WINDOW
                    | EventMask::COLOR_MAP_CHANGE,
            )
            .colormap(self.colormap.clone())
            .border_pixel(0x0);

        if windowgeom.depth == 24 {
            self.conn
                .as_ref()
                .expect("Client has no valid X connection")
                .create_window(
                    windowgeom.depth,
                    frameid,
                    self.screen.clone().unwrap().root,
                    windowgeom.x,
                    windowgeom.x,
                    windowgeom.width,
                    windowgeom.height,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    window_attrs.visual,
                    &winaux,
                )
                .expect("Failed to create client window");
        } else {
            self.conn
                .as_ref()
                .expect("Client has no valid X connection")
                .create_window(
                    mydepth,
                    frameid,
                    self.screen.clone().unwrap().root,
                    windowgeom.x,
                    windowgeom.x,
                    windowgeom.width,
                    windowgeom.height,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    myvisual,
                    &winaux,
                )
                .expect("Failed to create client window");
        }
        //add client window to save set in case WM crashes
        self.conn
            .as_ref()
            .expect("Client has no valid X Connection")
            .change_save_set(SetMode::INSERT, self.windowid)
            .expect("Failed to add client to save set");

        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .map_window(frameid)
            .expect("Failed to map frame");
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .reparent_window(self.windowid, frameid, 0, 0)
            .expect("Failed to reparent window");
        self.frameid = Some(frameid);
        //xcb uses u16 and u32 interchangably for geometry :(
        self.framewidth = windowgeom.width as u32;
        self.frameheight = windowgeom.height as u32;
        self.x = windowgeom.x as u32;
        self.y = windowgeom.y as u32;
        return true;
    }
    pub fn map_and_frame(&mut self) {
        self.frame(false);
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .map_window(self.frameid.unwrap())
            .expect("Failed to map window");
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .map_window(self.windowid)
            .expect("Failed to map window");
        self.mapped = true;
    }
    pub fn map(&mut self) {
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .map_window(self.frameid.unwrap())
            .expect("Failed to map window");
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .map_window(self.windowid)
            .expect("Failed to map window");
    }
    pub fn unframe(&mut self) {
        if !self.mapped {
            return;
        }
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .reparent_window(self.windowid, self.screen.as_ref().unwrap().root, 0, 0)
            .expect("Failed to reparent window");
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .unmap_window(self.frameid.unwrap())
            .expect("Failed to unmap window");

        //remove client window from SaveSetMode
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .change_save_set(SetMode::DELETE, self.windowid)
            .expect("Failed to remove client from save set");
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .destroy_window(self.frameid.unwrap())
            .expect("Failed to destroy window");

        self.mapped = false;
        self.frameid = None;
        self.framewidth = 0;
        self.frameheight = 0;
        self.x = 0;
        self.y = 0;
    }

    pub fn set_screen(&mut self, screen: Screen) {
        self.screen = Some(screen);
    }
    pub fn configure(&mut self, config: &ConfigureRequestEvent) {
        let mut aux = ConfigureWindowAux::from_configure_request(config)
            .sibling(None)
            .stack_mode(None);

        if self.floating == false {
            // we will ignore geometry config requests from the client unless window is in floating
            // mode. This is a dynamic window
            // manager after all

            aux = ConfigureWindowAux::from_configure_request(config)
                .x(None)
                .y(None)
                .width(None)
                .height(None);
        }
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .configure_window(self.windowid, &aux)
            .expect("Failed to configure windows;");
        let mut confnotify = ConfigureNotifyEvent::default();
        confnotify.height = self.frameheight as u16;
        confnotify.width = self.framewidth as u16;
        confnotify.response_type = CONFIGURE_NOTIFY_EVENT;

        if self.mapped {
            confnotify.above_sibling = self.frameid.unwrap();
        }
        confnotify.event = self.windowid;
        self.conn
            .as_ref()
            .expect("Client has no valid x connection")
            .send_event(true, self.windowid, EventMask::STRUCTURE_NOTIFY, confnotify)
            .unwrap();
    }

    pub fn move_resize(&self, geom: Geom) {
        //move_resize frame
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .configure_window(
                self.frameid.unwrap(),
                &ConfigureWindowAux::default()
                    .x(Some(geom.x as i32))
                    .y(Some(geom.y as i32))
                    .width(Some(geom.width as u32))
                    .height(Some(geom.height as u32)),
            )
            .expect("Failed to configure windows");
        //resize client
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .configure_window(
                self.windowid,
                &ConfigureWindowAux::default()
                    .width(Some(geom.width as u32))
                    .height(Some(geom.height as u32)),
            )
            .unwrap();
        let mut confnotify = ConfigureNotifyEvent::default();
        confnotify.height = self.frameheight as u16;
        confnotify.width = self.framewidth as u16;
        confnotify.response_type = CONFIGURE_NOTIFY_EVENT;
        confnotify.above_sibling = self.frameid.unwrap();
        confnotify.event = self.windowid;

        //tell client is has been resized

        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .send_event(
                true,
                self.windowid,
                EventMask::STRUCTURE_NOTIFY,
                &confnotify,
            )
            .unwrap();
    }

    pub fn get_frameid(&self) -> Window {
        //this is to get the frame id to grab keys on the frame. will do this later using config
        return self.frameid.clone().unwrap();
    }
}
