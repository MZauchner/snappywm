use crate::layout;
use image::io::Reader as ImageReader;
use keycode;
use rgb::alt::BGRA8;
use rgb::{self, AsPixels, ComponentBytes, ComponentSlice, FromSlice};
use std::io::{Cursor, Write};
use std::process;
use std::thread::sleep_ms;
use x11rb::atom_manager;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol::render::{self, ConnectionExt as _, PictType};
use x11rb::protocol::xfixes::SaveSetMode;
use x11rb::protocol::xkb::ConnectionExt;

use x11rb::protocol::xproto::*;
use x11rb::protocol::xproto::{ConnectionExt as _, *};
use x11rb::rust_connection::*;
atom_manager! {
    pub AtomCollection: AtomCollectionCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        _NET_WM_NAME,
        UTF8_STRING,
    }
}
fn get_key_code(key: keycode::KeyMappingId) -> u8 {
    let code: u8 = (keycode::KeyMap::from(key).xkb).try_into().unwrap();
    return code;
}
struct FrameWinPair {
    frame: Window,
    window: Window,
}
pub struct WindowManager {
    conn: Box<RustConnection>,
    screen_nums: Vec<usize>,
    screens: Vec<Screen>,
    windows: Vec<FrameWinPair>,
    layout: Box<dyn layout::Layout>,
}

impl WindowManager {
    pub fn new() -> Box<WindowManager> {
        let (connection, screen_num) = RustConnection::connect(None).unwrap();
        let screen = &connection.setup().roots[screen_num];
        let screens_int = vec![screen.to_owned()];
        let screens_nums_int = vec![screen_num];
        let params = connection
            .get_geometry(screen.root)
            .unwrap()
            .reply()
            .unwrap();

        return Box::new(WindowManager {
            conn: Box::new(connection),
            screen_nums: screens_nums_int,
            screens: screens_int,
            windows: vec![],
            layout: layout::MasterSlave::new(layout::RootParams {
                width: params.width,
                height: params.height,
            }),
        });
    }
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let confaux = ChangeWindowAttributesAux::default()
            .event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, //   | EventMask::KEY_PRESS,
            )
            .background_pixel(self.screens[0].white_pixel);

        let res = self
            .conn
            .change_window_attributes(self.screens[0].root, &confaux)?
            .check();

        //handle windows created before launch of wm
        //

        self.conn.grab_server().unwrap().check().unwrap();
        let query_tree = self
            .conn
            .query_tree(self.screens[0].root)
            .unwrap()
            .reply()
            .unwrap();
        for child in query_tree.children {
            self.frame(&child, true);
        }

        self.conn.ungrab_server().unwrap().check().unwrap();
        let res = self
            .conn
            .grab_key(
                true,
                self.screens[0].root,
                x11rb::protocol::xproto::ModMask::M4,
                get_key_code(keycode::KeyMappingId::UsA),
                x11rb::protocol::xproto::GrabMode::ASYNC,
                x11rb::protocol::xproto::GrabMode::ASYNC,
            )?
            .check();

        self.conn.flush()?;

        loop {
            self.conn.flush()?;
            let ev = self.conn.wait_for_event()?;
            match ev {
                x11rb::protocol::Event::CreateNotify(event) => self.on_create_notify(event),
                x11rb::protocol::Event::ConfigureRequest(event) => self.on_configure_request(event),
                x11rb::protocol::Event::ConfigureNotify(event) => self.on_configure_notify(event),
                x11rb::protocol::Event::DestroyNotify(event) => self.on_destroy_notify(event),
                x11rb::protocol::Event::ReparentNotify(event) => self.on_reparent_notify(event),
                x11rb::protocol::Event::KeyPress(event) => self.on_key_press(event),
                x11rb::protocol::Event::MapRequest(event) => self.on_map_request(event),
                x11rb::protocol::Event::MapNotify(event) => self.on_map_notify(event),
                x11rb::protocol::Event::UnmapNotify(event) => self.on_unmap_notify(event),
                _ => {
                    println!("{:?}", ev);
                }
            }
        }
    }
    fn on_create_notify(&self, ev: CreateNotifyEvent) {}
    fn on_destroy_notify(&self, ev: DestroyNotifyEvent) {}
    fn on_map_request(&mut self, ev: MapRequestEvent) {
        println!("{:?}", ev);
        self.layout.push();
        let frame = self.frame(&ev.window, false);
        self.conn.map_window(frame).unwrap();
        self.conn.map_window(ev.window).unwrap();
        self.conn.flush().unwrap();
        self.windows
            .iter()
            .rev()
            .map(|a| {
                let geom = self.layout.next_geom();
                println!("{:#?}", geom);
                let confauxauxframe = ConfigureWindowAux::default()
                    .x(Some(i32::from(geom.x)))
                    .y(Some(i32::from(geom.y)))
                    .width(Some(u32::from(geom.width)))
                    .height(Some(u32::from(geom.height)));
                let confauxauxwin = ConfigureWindowAux::default()
                    .width(Some(u32::from(geom.width)))
                    .height(Some(u32::from(geom.height)));
                let mut event = ConfigureNotifyEvent::default();
                event.event = a.window;
                event.above_sibling = a.frame;
                event.width = geom.width as u16;
                event.height = geom.height as u16;
                event.response_type = CONFIGURE_NOTIFY_EVENT;

                self.conn
                    .send_event(true, a.window, EventMask::SUBSTRUCTURE_NOTIFY, event)
                    .unwrap();
                self.conn
                    .configure_window(a.frame, &confauxauxframe)
                    .unwrap();
                self.conn
                    .configure_window(a.window, &confauxauxwin)
                    .unwrap();
                self.conn.flush().unwrap();
                FrameWinPair {
                    frame: a.frame,
                    window: a.window,
                }
            })
            .count();
        let res = self
            .conn
            .grab_key(
                true,
                frame,
                x11rb::protocol::xproto::ModMask::M4,
                get_key_code(keycode::KeyMappingId::UsA),
                x11rb::protocol::xproto::GrabMode::ASYNC,
                x11rb::protocol::xproto::GrabMode::ASYNC,
            )
            .unwrap()
            .check();

        self.conn.flush().unwrap();
    }
    fn on_map_notify(&self, ev: MapNotifyEvent) {}
    fn on_configure_notify(&self, ev: ConfigureNotifyEvent) {}
    fn on_configure_request(&mut self, ev: ConfigureRequestEvent) {
        println!("{:?}", ev);
        let attrs = self
            .conn
            .get_geometry(self.screens[0].root)
            .unwrap()
            .reply()
            .unwrap();
        let config = ConfigureWindowAux::from_configure_request(&ev)
            .sibling(None)
            .stack_mode(None);

        self.conn
            .configure_window(ev.window.clone(), &config)
            .unwrap();
        //resize frame of window
        self.windows
            .iter()
            .filter(|a| a.window == ev.window)
            .map(|a| {
                self.conn.configure_window(a.frame, &config).unwrap();
                FrameWinPair {
                    frame: a.frame.clone(),
                    window: a.window.clone(),
                }
            })
            .count();
    }
    fn on_unmap_notify(&mut self, ev: UnmapNotifyEvent) {
        println!("{:?}", ev);
        //ignore event if it was triggered by reparenting a window that was created before
        //launching the wm
        if ev.event == self.screens[0].root {
            return;
        }
        let frame: &FrameWinPair = self.windows.iter().find(|a| a.window == ev.window).unwrap();
        self.conn.unmap_window(frame.frame).unwrap();
        self.conn
            .reparent_window(frame.window, self.screens[0].root, 0, 0)
            .unwrap();
        self.conn
            .change_save_set(SetMode::DELETE, frame.frame)
            .unwrap();

        self.conn.destroy_window(frame.frame).unwrap();
        self.windows.retain(|a| a.window != ev.window);
        self.layout.pop();
    }

    fn on_reparent_notify(&self, ev: ReparentNotifyEvent) {}
    fn on_key_press(&self, ev: KeyPressEvent) {
        println!("{:?}", ev);
        process::Command::new("/usr/bin/alacritty").spawn().unwrap();
    }
    fn frame(&mut self, window: &Window, created_before_wm: bool) -> u32 {
        let frameid = self.conn.generate_id().unwrap();
        let window_attrs = self
            .conn
            .get_window_attributes(*window)
            .unwrap()
            .reply()
            .unwrap();
        if created_before_wm {
            if window_attrs.override_redirect || window_attrs.map_state != MapState::VIEWABLE {
                return 0;
            }
        }
        let window_geom = self.conn.get_geometry(*window).unwrap().reply().unwrap();
        let frameaux = CreateWindowAux::default()
            .event_mask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT)
            .border_pixel(self.screens[0].white_pixel);
        self.conn
            .create_window(
                self.screens[0].root_depth,
                frameid,
                self.screens[0].root,
                window_geom.x,
                window_geom.y,
                window_geom.width,
                window_geom.height,
                0,
                window_attrs.class,
                window_attrs.visual,
                &frameaux,
            )
            .unwrap();
        self.conn.change_save_set(SetMode::INSERT, *window).unwrap();
        self.conn.reparent_window(*window, frameid, 0, 0).unwrap();
        self.windows.push(FrameWinPair {
            frame: frameid,
            window: *window,
        });
        frameid
    }

    fn on_window_manager_detected() {}
    fn on_x_error() {}
}
