//! A small 2D game kit: gravity, solid bodies, and tagged hitboxes.
//!
//! Designed to be readable from PlainText (`import gamekit`). Physics is
//! axis-aligned boxes only — no slopes, rotation, or joints. Units are pixels
//! and seconds (`delta` from `on update`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// A rectangle body in the world. Position is the top-left corner.
#[derive(Clone)]
pub struct Body {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub vx: f64,
    pub vy: f64,
    /// Participates in blocking collision with other solids.
    pub solid: bool,
    /// If true, never moved by gravity or velocity during `world.step`
    /// (platforms, walls). Still blocks dynamic solids.
    pub is_static: bool,
    /// Set by `step` when resting on a solid below.
    pub on_ground: bool,
}

impl Body {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Body {
        Body {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
            vx: 0.0,
            vy: 0.0,
            solid: true,
            is_static: false,
            on_ground: false,
        }
    }

    pub fn center_x(&self) -> f64 { self.x + self.width / 2.0 }
    pub fn center_y(&self) -> f64 { self.y + self.height / 2.0 }

    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    pub fn set_velocity(&mut self, vx: f64, vy: f64) {
        self.vx = vx;
        self.vy = vy;
    }

    pub fn bump(&mut self, vx: f64, vy: f64) {
        self.vx += vx;
        self.vy += vy;
    }

    /// Upward jump. Only applies when `on_ground` (unless `force`).
    pub fn jump(&mut self, speed: f64, force: bool) -> bool {
        if force || self.on_ground {
            self.vy = -speed.abs();
            self.on_ground = false;
            true
        } else {
            false
        }
    }
}

/// A rectangle attached to a body (or free-floating if `owner` is none).
#[derive(Clone)]
pub struct Hitbox {
    pub owner: Option<Rc<RefCell<Body>>>,
    pub offset_x: f64,
    pub offset_y: f64,
    pub width: f64,
    pub height: f64,
    pub kind: String,
    pub active: bool,
}

impl Hitbox {
    pub fn new(
        owner: Option<Rc<RefCell<Body>>>,
        offset_x: f64,
        offset_y: f64,
        width: f64,
        height: f64,
        kind: String,
        active: bool,
    ) -> Hitbox {
        Hitbox {
            owner,
            offset_x,
            offset_y,
            width: width.max(0.0),
            height: height.max(0.0),
            kind,
            active,
        }
    }

    /// World-space top-left.
    pub fn world_xy(&self) -> (f64, f64) {
        if let Some(owner) = &self.owner {
            let b = owner.borrow();
            (b.x + self.offset_x, b.y + self.offset_y)
        } else {
            (self.offset_x, self.offset_y)
        }
    }

    pub fn world_rect(&self) -> (f64, f64, f64, f64) {
        let (x, y) = self.world_xy();
        (x, y, self.width, self.height)
    }
}

pub fn rects_overlap(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> bool {
    ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
}

pub fn hitboxes_overlap(a: &Hitbox, b: &Hitbox) -> bool {
    if !a.active || !b.active {
        return false;
    }
    let (ax, ay, aw, ah) = a.world_rect();
    let (bx, by, bw, bh) = b.world_rect();
    rects_overlap(ax, ay, aw, ah, bx, by, bw, bh)
}

/// Physics world: gravity, solid resolution, and one-shot hit tracking.
pub struct World {
    pub gravity: f64,
    bodies: Vec<Rc<RefCell<Body>>>,
    hitboxes: Vec<Rc<RefCell<Hitbox>>>,
    /// (attack ptr, hurt ptr) pairs that already scored while the attack stayed active.
    hit_pairs: HashSet<(usize, usize)>,
}

impl World {
    pub fn new(gravity: f64) -> World {
        World {
            gravity,
            bodies: Vec::new(),
            hitboxes: Vec::new(),
            hit_pairs: HashSet::new(),
        }
    }

    pub fn add_body(&mut self, body: Rc<RefCell<Body>>) {
        if !self.bodies.iter().any(|b| Rc::ptr_eq(b, &body)) {
            self.bodies.push(body);
        }
    }

    pub fn add_hitbox(&mut self, hb: Rc<RefCell<Hitbox>>) {
        if !self.hitboxes.iter().any(|h| Rc::ptr_eq(h, &hb)) {
            self.hitboxes.push(hb);
        }
    }

    pub fn hitboxes(&self) -> &[Rc<RefCell<Hitbox>>] {
        &self.hitboxes
    }

    /// Apply gravity, integrate velocity, resolve solid AABB collisions.
    pub fn step(&mut self, delta: f64) {
        let dt = delta.max(0.0).min(0.1); // clamp a long hitch
        let n = self.bodies.len();

        for i in 0..n {
            let mut body = self.bodies[i].borrow_mut();
            if body.is_static {
                body.on_ground = false;
                continue;
            }
            body.vy += self.gravity * dt;

            // Horizontal move + resolve.
            body.x += body.vx * dt;
            drop(body);
            self.resolve_axis(i, true);

            // Vertical move + resolve.
            let mut body = self.bodies[i].borrow_mut();
            body.y += body.vy * dt;
            body.on_ground = false;
            drop(body);
            self.resolve_axis(i, false);
        }

        // Drop hit-pairs for attacks that are no longer active.
        self.prune_hit_pairs();
    }

    fn resolve_axis(&mut self, i: usize, horizontal: bool) {
        let n = self.bodies.len();
        for j in 0..n {
            if i == j {
                continue;
            }
            // Borrow both carefully.
            let (a_rc, b_rc) = (self.bodies[i].clone(), self.bodies[j].clone());
            let mut a = a_rc.borrow_mut();
            let b = b_rc.borrow();
            if !a.solid || !b.solid {
                continue;
            }
            if !rects_overlap(a.x, a.y, a.width, a.height, b.x, b.y, b.width, b.height) {
                continue;
            }

            if horizontal {
                let a_cx = a.center_x();
                let b_cx = b.center_x();
                if a_cx < b_cx {
                    a.x = b.x - a.width;
                } else {
                    a.x = b.x + b.width;
                }
                a.vx = 0.0;
            } else {
                let a_cy = a.center_y();
                let b_cy = b.center_y();
                if a_cy < b_cy {
                    // Landing on top of b.
                    a.y = b.y - a.height;
                    if a.vy >= 0.0 {
                        a.vy = 0.0;
                        a.on_ground = true;
                    }
                } else {
                    // Hit underside.
                    a.y = b.y + b.height;
                    if a.vy < 0.0 {
                        a.vy = 0.0;
                    }
                }
            }
        }
    }

    fn prune_hit_pairs(&mut self) {
        let active_attacks: HashSet<usize> = self
            .hitboxes
            .iter()
            .filter(|h| h.borrow().active)
            .map(|h| Rc::as_ptr(h) as usize)
            .collect();
        self.hit_pairs.retain(|(a, _)| active_attacks.contains(a));
    }

    /// True the first time `attack` overlaps `hurt` while `attack` is active.
    /// Stays false until `attack` turns inactive (then can fire again).
    pub fn hits(&mut self, attack: &Rc<RefCell<Hitbox>>, hurt: &Rc<RefCell<Hitbox>>) -> bool {
        let a = attack.borrow();
        let h = hurt.borrow();
        if !a.active {
            let ap = Rc::as_ptr(attack) as usize;
            self.hit_pairs.retain(|(x, _)| *x != ap);
            return false;
        }
        if !hitboxes_overlap(&a, &h) {
            return false;
        }
        let key = (Rc::as_ptr(attack) as usize, Rc::as_ptr(hurt) as usize);
        if self.hit_pairs.contains(&key) {
            return false;
        }
        self.hit_pairs.insert(key);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_lands_on_platform() {
        let mut world = World::new(2000.0);
        let hero = Rc::new(RefCell::new(Body::new(10.0, 0.0, 20.0, 30.0)));
        let mut ground = Body::new(0.0, 200.0, 400.0, 40.0);
        ground.is_static = true;
        let ground = Rc::new(RefCell::new(ground));
        world.add_body(hero.clone());
        world.add_body(ground);
        for _ in 0..120 {
            world.step(1.0 / 60.0);
        }
        let h = hero.borrow();
        assert!(h.on_ground, "expected on ground, y={}", h.y);
        assert!((h.y + h.height - 200.0).abs() < 0.5, "y={}", h.y);
    }

    #[test]
    fn hits_once_per_swing() {
        let mut world = World::new(0.0);
        let a_body = Rc::new(RefCell::new(Body::new(0.0, 0.0, 10.0, 10.0)));
        let b_body = Rc::new(RefCell::new(Body::new(5.0, 0.0, 10.0, 10.0)));
        let attack = Rc::new(RefCell::new(Hitbox::new(
            Some(a_body), 0.0, 0.0, 10.0, 10.0, "attack".into(), true,
        )));
        let hurt = Rc::new(RefCell::new(Hitbox::new(
            Some(b_body), 0.0, 0.0, 10.0, 10.0, "hurt".into(), true,
        )));
        assert!(world.hits(&attack, &hurt));
        assert!(!world.hits(&attack, &hurt));
        attack.borrow_mut().active = false;
        assert!(!world.hits(&attack, &hurt));
        attack.borrow_mut().active = true;
        assert!(world.hits(&attack, &hurt));
    }
}
