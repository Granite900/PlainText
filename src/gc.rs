//! A mark-and-sweep cycle collector layered on the `Rc` heap.
//!
//! PlainText's values are reference counted (`Rc`), which already frees
//! everything the moment nothing points at it — *except* reference cycles (a
//! list that contains itself, two objects that point at each other). Those
//! keep each other's counts above zero forever, so they leak.
//!
//! This collector fixes that. Every mutable heap object (list, dictionary,
//! class instance) registers a weak handle here when it's created. Every so
//! often the interpreter runs [`Heap::collect`]: it marks everything reachable
//! from the roots (globals, active call frames, timers, loop snapshots), then
//! for any registered object that wasn't reached — it must be stuck in a cycle,
//! since a truly unreferenced object would already be gone — it clears the
//! object's contents. That breaks the cycle, the reference counts fall to zero,
//! and `Rc` reclaims the memory.
//!
//! Scopes are traced (to find reachable objects) but never cleared, so a live
//! local can never be corrupted — the worst a missed root could do is keep
//! garbage a little longer, never free something still in use.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use crate::value::{scope_snapshot, ClassInstance, Env, PtMap, Value};

/// A weak handle to one collectable object.
enum GcObj {
    List(Weak<RefCell<Vec<Value>>>),
    Dict(Weak<RefCell<PtMap>>),
    Instance(Weak<RefCell<ClassInstance>>),
}

impl GcObj {
    fn alive(&self) -> bool {
        match self {
            GcObj::List(w) => w.strong_count() > 0,
            GcObj::Dict(w) => w.strong_count() > 0,
            GcObj::Instance(w) => w.strong_count() > 0,
        }
    }
}

pub struct Heap {
    objects: Vec<GcObj>,
    /// Allocations since the last collection.
    since_gc: usize,
    /// Allocation count that triggers the next collection.
    next_gc: usize,
    /// Lower bound for the trigger (set low via PT_GC_STRESS to test the GC).
    floor: usize,
}

impl Heap {
    pub fn new() -> Heap {
        let floor = std::env::var("PT_GC_STRESS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512);
        Heap { objects: Vec::new(), since_gc: 0, next_gc: floor, floor }
    }

    pub fn track_list(&mut self, rc: &Rc<RefCell<Vec<Value>>>) {
        self.objects.push(GcObj::List(Rc::downgrade(rc)));
        self.since_gc += 1;
    }

    pub fn track_dict(&mut self, rc: &Rc<RefCell<PtMap>>) {
        self.objects.push(GcObj::Dict(Rc::downgrade(rc)));
        self.since_gc += 1;
    }

    pub fn track_instance(&mut self, rc: &Rc<RefCell<ClassInstance>>) {
        self.objects.push(GcObj::Instance(Rc::downgrade(rc)));
        self.since_gc += 1;
    }

    pub fn should_collect(&self) -> bool {
        self.since_gc >= self.next_gc
    }

    /// Mark from the roots, then clear any unreachable (cyclic) object. Returns
    /// how many objects were swept (for diagnostics/tests).
    pub fn collect(&mut self, roots: &[Value], root_envs: &[Env]) -> usize {
        let mut marks: HashSet<usize> = HashSet::new();
        for v in roots {
            mark_value(v, &mut marks);
        }
        for e in root_envs {
            mark_env(e, &mut marks);
        }

        let mut swept = 0;
        for obj in &self.objects {
            match obj {
                GcObj::List(w) => {
                    if let Some(rc) = w.upgrade() {
                        if !marks.contains(&ptr_of(&rc)) {
                            rc.borrow_mut().clear();
                            swept += 1;
                        }
                    }
                }
                GcObj::Dict(w) => {
                    if let Some(rc) = w.upgrade() {
                        if !marks.contains(&ptr_of(&rc)) {
                            rc.borrow_mut().clear();
                            swept += 1;
                        }
                    }
                }
                GcObj::Instance(w) => {
                    if let Some(rc) = w.upgrade() {
                        if !marks.contains(&ptr_of(&rc)) {
                            rc.borrow_mut().fields.clear();
                            swept += 1;
                        }
                    }
                }
            }
        }

        // Forget handles whose objects are now gone, and schedule the next run
        // relative to how much is still live.
        self.objects.retain(|o| o.alive());
        self.since_gc = 0;
        self.next_gc = (self.objects.len() * 2).max(self.floor);
        swept
    }
}

fn ptr_of<T>(rc: &Rc<T>) -> usize {
    Rc::as_ptr(rc) as *const () as usize
}

/// Recursively mark every heap object reachable from `v`.
fn mark_value(v: &Value, marks: &mut HashSet<usize>) {
    match v {
        Value::List(rc) => {
            if marks.insert(ptr_of(rc)) {
                for item in rc.borrow().iter() {
                    mark_value(item, marks);
                }
            }
        }
        Value::Dictionary(rc) => {
            if marks.insert(ptr_of(rc)) {
                for (_, val) in rc.borrow().entries.iter() {
                    mark_value(val, marks);
                }
            }
        }
        Value::Class(rc) => {
            if marks.insert(ptr_of(rc)) {
                for val in rc.borrow().fields.values() {
                    mark_value(val, marks);
                }
            }
        }
        Value::Function(f) => mark_env(&f.closure, marks),
        Value::BoundMethod { receiver, func } => {
            mark_value(receiver, marks);
            mark_env(&func.closure, marks);
        }
        // Numbers, text, booleans and builtins can't hold cycles.
        _ => {}
    }
}

/// Mark everything reachable through a scope chain (values + parent scopes).
fn mark_env(env: &Env, marks: &mut HashSet<usize>) {
    // Envs share the pointer-mark set only to avoid re-tracing the same scope;
    // scope pointers are distinct allocations from value pointers, so they
    // never collide with the objects the sweep looks at.
    let key = Rc::as_ptr(env) as *const () as usize;
    if !marks.insert(key) {
        return;
    }
    let (values, parent) = scope_snapshot(env);
    for v in &values {
        mark_value(v, marks);
    }
    if let Some(par) = parent {
        mark_env(&par, marks);
    }
}
