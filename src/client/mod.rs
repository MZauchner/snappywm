use std::rc::Rc;

use crate::layout::Geom;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use x11rb::rust_connection::*;

pub struct Client {
    conn: Option<Rc<RustConnection>>,
    pub frameid: Option<u32>,
    pub windowid: u32,
    x: u32,
    y: u32,
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
        let winaux = CreateWindowAux::default()
            .event_mask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT);
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .create_window(
                parent.root_depth,
                frameid,
                parent.root,
                windowgeom.x,
                windowgeom.x,
                windowgeom.width,
                windowgeom.height,
                0,
                window_attrs.class,
                window_attrs.visual,
                &winaux,
            )
            .expect("Failed to create client window");
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
        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .unmap_window(self.frameid.unwrap())
            .expect("Failed to unmap window");

        self.conn
            .as_ref()
            .expect("Client has no valid X connection")
            .reparent_window(self.windowid, self.screen.as_ref().unwrap().root, 0, 0)
            .expect("Failed to reparent window");

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
