use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

use uuid::Uuid;

use crate::network::edge::EdgeKind;

thread_local! {
    static EDGE_ANIMS: RefCell<HashMap<EdgeKey, EdgeAnimation>> = RefCell::new(HashMap::new())
}

pub fn publish_create(src: Uuid, dst: Uuid, kind: EdgeKind) {
    EDGE_ANIMS.with(|m| {
        m.borrow_mut().insert(EdgeKey { src, dst, kind }, EdgeAnimation::new_creating());
    });
}

pub fn publish_destroy(src: Uuid, dst: Uuid, kind: EdgeKind) {
    EDGE_ANIMS.with(|m| {
        m.borrow_mut().insert(EdgeKey { src, dst, kind }, EdgeAnimation::new_destroying());
    });
}

pub fn get_anim(src: Uuid, dst: Uuid, kind: EdgeKind) -> Option<EdgeAnimation> {
    EDGE_ANIMS.with(|m| m.borrow().get(&EdgeKey { src, dst, kind }).cloned())
}

pub fn cleanup_finished(total: std::time::Duration) {
    let now = Instant::now();
    EDGE_ANIMS.with(|m| {
        m.borrow_mut().retain(|_, anim| {
            let done = match anim.phase {
                EdgeAnimPhase::Creating => anim.start_time.elapsed() >= total,
                EdgeAnimPhase::Destroying => anim.start_time.elapsed() >= total,
            };
            !done
        });
    });
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct EdgeKey {
    src: Uuid,
    dst: Uuid,
    kind: EdgeKind
}

#[derive(Clone, Debug)]
pub enum EdgeAnimPhase {
    Creating,
    Destroying
}

#[derive(Clone, Debug)]
pub struct EdgeAnimation {
    pub phase: EdgeAnimPhase,
    pub start_time: Instant
}

impl EdgeAnimation {
    pub fn new_creating() -> Self {
        Self {
            phase: EdgeAnimPhase::Creating,
            start_time: Instant::now()
        }
    }
    
    pub fn new_destroying() -> Self {
        Self {
            phase: EdgeAnimPhase::Destroying,
            start_time: Instant::now()
        }
    }
    
    pub fn linear_progress(&self, total: Duration) -> f32 {
        (self.start_time.elapsed().as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    }
    
    pub fn eased_progress<F>(&self, total: Duration, easing: F) -> f32 
    where
        F: Fn(f32) -> f32
    {
        easing(self.linear_progress(total))
    }
}