use std::borrow::Borrow;

pub trait Layout {
    fn push(&mut self);
    fn pop(&mut self);
    fn next_geom(&mut self) -> Geom;
    fn reset(&mut self);
}
pub struct RootParams {
    pub width: u16,
    pub height: u16,
}
#[derive(Debug, Clone)]
pub struct Geom {
    pub width: u16,
    pub height: u16,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug)]
pub struct Spiral {
    cur: usize,
    len: usize,
    rootheight: u16,
    rootwidth: u16,
    geoms: Vec<Geom>,
}
#[derive(Debug)]
pub struct MasterSlave {
    cur: usize,
    len: usize,
    rootheight: u16,
    rootwidth: u16,
    geoms: Vec<Geom>,
}

impl Spiral {
    pub fn new(root_params: RootParams) -> Box<Spiral> {
        Box::new(Spiral {
            cur: 0,
            len: 0,
            rootwidth: root_params.width,
            rootheight: root_params.height,
            geoms: Vec::<Geom>::new(),
        })
    }
}
impl MasterSlave {
    pub fn new(root_params: RootParams) -> Box<MasterSlave> {
        Box::new(MasterSlave {
            cur: 0,
            len: 0,
            rootwidth: root_params.width,
            rootheight: root_params.height,
            geoms: Vec::<Geom>::new(),
        })
    }
}
impl Layout for Spiral {
    fn reset(&mut self) {
        self.cur = 0;
        self.geoms.clear();
        self.geoms.reserve(self.len);
        if self.len == 1 {
            self.geoms.push(Geom {
                width: self.rootwidth,
                height: self.rootheight,
                x: 0,
                y: 0,
            });
        } else {
            for i in 0..self.len {
                match i {
                    0 => {
                        self.geoms.push(Geom {
                            width: self.rootwidth / 2,
                            height: self.rootheight,
                            x: 0,
                            y: 0,
                        });
                    }
                    _ => {
                        if i % 2 == 0 {
                            if i != self.len - 1 {
                                self.geoms.push(Geom {
                                    width: self.geoms[i - 1].width / 2,
                                    height: self.geoms[i - 1].height,
                                    x: 0,
                                    y: 0,
                                });
                            } else {
                                self.geoms.push(Geom {
                                    width: self.geoms[i - 1].width,
                                    height: self.geoms[i - 1].height,
                                    x: self.geoms[i - 1].x,
                                    y: self.geoms[i - 1].y + self.geoms[i - 1].height,
                                });
                            }
                        } else if i % 2 == 1 {
                            if i != self.len - 1 {
                                self.geoms.push(Geom {
                                    width: self.geoms[i - 1].width,
                                    height: self.geoms[i - 1].height / 2,
                                    x: 0,
                                    y: 0,
                                });
                            } else {
                                self.geoms.push(Geom {
                                    width: self.geoms[i - 1].width,
                                    height: self.geoms[i - 1].height,
                                    x: self.geoms[i - 1].x + self.geoms[i - 1].width,
                                    y: self.geoms[i - 1].y,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    fn push(&mut self) {
        self.len += 1;
        self.reset();
    }

    fn pop(&mut self) {
        self.len -= 1;
        self.reset();
    }
    fn next_geom(&mut self) -> Geom {
        let cur = self.cur.clone();
        self.cur += 1;
        if self.cur == self.len {
            self.cur = 0;
        }

        return self.geoms[cur].clone();
    }
}
impl Layout for MasterSlave {
    fn reset(&mut self) {
        self.geoms.clear();
        self.geoms.reserve(self.len);
        if self.len == 1 {
            self.geoms.push(Geom {
                width: self.rootwidth.clone(),
                height: self.rootheight.clone(),
                x: 0,
                y: 0,
            });
        } else {
            let mut delta = usize::from(self.rootheight);
            if self.len > 1 {
                delta = delta / (self.len - 1);
            }
            let delta = delta as u16;
            let len = self.len as u16;
            for i in 0..self.len {
                if i == 0 {
                    self.geoms.push(Geom {
                        width: 2 * self.rootwidth.clone() / 3,
                        height: self.rootheight.clone(),
                        x: 0,
                        y: 0,
                    });
                } else {
                    if i > 1 {
                        self.geoms.push(Geom {
                            width: self.rootwidth.clone() / 3,
                            height: self.rootheight.clone() / (len - 1),
                            x: 2 * self.rootwidth.clone() / 3,
                            y: self.geoms[i - 1].y + delta,
                        });
                    } else {
                        self.geoms.push(Geom {
                            width: 1 * self.rootwidth.clone() / 3,
                            height: self.rootheight.clone() / (len - 1),
                            x: 2 * self.rootwidth.clone() / 3,
                            y: 0,
                        })
                    }
                }
            }
        }
    }
    fn push(&mut self) {
        println!("push");
        self.len += 1;
        self.reset();
    }

    fn pop(&mut self) {
        println!("pop");
        self.len -= 1;
        self.reset();
    }
    fn next_geom(&mut self) -> Geom {
        println!("cur: {:?}", *self);
        let mut cur = self.cur.clone();
        self.cur += 1;
        if self.cur == self.len {
            self.cur = 0;
        }

        return self.geoms[cur].clone();
    }
}
