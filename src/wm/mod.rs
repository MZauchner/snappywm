use crate::client;
use crate::layout;
use keycode;
use std::process;
use std::rc::Rc;
use x11rb::atom_manager;
use x11rb::connection::Connection;

use x11rb::protocol::xproto::*;
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
pub struct WindowManager {
    conn: Rc<RustConnection>,
    screens: Vec<Screen>,
    clients: Vec<Box<client::Client>>,
    layout: Box<dyn layout::Layout>,
}

impl WindowManager {
    pub fn new() -> Box<WindowManager> {
        let (connection, screen_num) = RustConnection::connect(None).unwrap();
        let screens = connection.setup().roots.to_owned();
        let params = connection
            .get_geometry(screens[0].root)
            .unwrap()
            .reply()
            .unwrap();

        return Box::new(WindowManager {
            conn: Rc::new(connection),
            screens: screens.to_vec(),
            clients: Vec::<Box<client::Client>>::new(),
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
            .background_pixel(None);

        //handle windows created before launch of wm
        //

        self.conn.grab_server().unwrap().check().unwrap();
        let mut i = 0;
        for screen in self.screens.iter() {
            self.conn
                .change_window_attributes(screen.root, &confaux)?
                .check()?;
            println!("{:?}", i);
            i += 1;
            let query_tree = self.conn.query_tree(screen.root).unwrap().reply().unwrap();
            for child in query_tree.children {
                let mut client = client::Client::new(child, (*screen).clone(), self.conn.clone());
                let framed = client.frame(true);
                //grab keys -- this will later be done via config struct

                if framed {
                    self.layout.push();
                    client.map();
                    self.conn
                        .grab_key(
                            true,
                            client.get_frameid(),
                            x11rb::protocol::xproto::ModMask::M4,
                            get_key_code(keycode::KeyMappingId::UsA),
                            x11rb::protocol::xproto::GrabMode::ASYNC,
                            x11rb::protocol::xproto::GrabMode::ASYNC,
                        )?
                        .check()?;
                }
                self.clients.push(Box::new(*client));
            }
            self.conn
                .grab_key(
                    true,
                    screen.root,
                    x11rb::protocol::xproto::ModMask::M4,
                    get_key_code(keycode::KeyMappingId::UsA),
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                )?
                .check()?;
        }
        self.conn.ungrab_server().unwrap().check().unwrap();

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
    fn on_create_notify(&self, _ev: CreateNotifyEvent) {}
    fn on_destroy_notify(&self, ev: DestroyNotifyEvent) {
        println!("destroyed {:?}", ev);
    }
    fn on_map_request(&mut self, ev: MapRequestEvent) {
        println!("{:?}", ev);

        let win = ev.window as Window;
        if self
            //make sure client is definitely there
            .clients
            .iter()
            .filter(|a| a.windowid == ev.window)
            .count()
            == 0
        {
            let root = self.screens.iter().find(|a| a.root == ev.parent).unwrap();
            self.clients.push(*Box::new(client::Client::new(
                ev.window,
                root.clone(),
                self.conn.clone(),
            )))
        }

        self.clients
            .iter_mut()
            .filter(|a| a.windowid == win)
            .map(|a| {
                a.map_and_frame();
                println!("mapped");
                return 0;
            })
            .count();
        self.conn.flush().unwrap();
        self.layout.push();

        self.clients
            .iter()
            .rev()
            .filter(|a| a.mapped)
            .map(|a| {
                let geom = self.layout.next_geom();
                a.move_resize(geom.clone());

                println!("{:#?}", geom);
            })
            .count();
        let frameid: Vec<Window> = self
            .clients
            .iter()
            .filter(|a| a.windowid == win)
            .map(|a| a.get_frameid())
            .collect();
        let frameid = frameid[0];

        self.conn
            .grab_key(
                true,
                frameid,
                x11rb::protocol::xproto::ModMask::M4,
                get_key_code(keycode::KeyMappingId::UsA),
                x11rb::protocol::xproto::GrabMode::ASYNC,
                x11rb::protocol::xproto::GrabMode::ASYNC,
            )
            .unwrap();

        self.conn.flush().unwrap();
    }
    fn on_map_notify(&self, _ev: MapNotifyEvent) {}
    fn on_configure_notify(&self, _ev: ConfigureNotifyEvent) {}
    fn on_configure_request(&mut self, ev: ConfigureRequestEvent) {
        println!("{:?}", ev);
        if self
            .clients
            .iter()
            .filter(|a| a.windowid == ev.window)
            .count()
            == 0
        {
            let root = self
                .screens
                .iter()
                .find(|a| {
                    println!("{:?} , {:?}", a.root, ev.parent);
                    a.root == ev.parent
                })
                .unwrap();
            self.clients.push(*Box::new(client::Client::new(
                ev.window,
                root.clone(),
                self.conn.clone(),
            )))
        }

        self.clients
            .iter_mut()
            .filter(|a| a.windowid == ev.window)
            .map(|a| {
                a.configure(&ev);
                return 0;
            })
            .count();
        //resize frame of window
    }
    fn on_unmap_notify(&mut self, ev: UnmapNotifyEvent) {
        println!("{:?}", ev);
        //ignore event if it was triggered by reparenting a window that was created before
        //launching the wm
        let k = self.screens.iter().find(|a| a.root == ev.event);
        if let Some(_) = k {
            println!("HELLOooooooooooooooooooooooooooo {:?}", ev);
            return;
        }
        self.clients
            .iter_mut()
            .filter(|a| a.windowid == ev.window && a.mapped)
            .map(|a| {
                println!("HELLOooooooooooooooooooooooooooo UNMAPPPP {:?}", ev);
                if a.mapped {
                    self.layout.pop();
                }
                a.unframe();
                0
            })
            .count();
    }

    fn on_reparent_notify(&self, _ev: ReparentNotifyEvent) {}
    fn on_key_press(&self, ev: KeyPressEvent) {
        println!("{:?}", ev);
        process::Command::new("/usr/bin/alacritty").spawn().unwrap();
    }
}
