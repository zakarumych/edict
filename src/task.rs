//! Async task execution.

use core::{future::Future, pin::Pin};

use alloc::boxed::Box;

use crate::{
    borrow_dyn_trait,
    component::{Component, ComponentBorrow},
    world::World,
};

/// Component that wraps a future.
pub struct Task<F> {
    future: Pin<Box<F>>,
}

pub trait AnyTask: Send {}

impl<F> AnyTask for Task<F> where F: Future + Send {}

impl<F> Component for Task<F>
where
    F: Future + 'static,
{
    fn name() -> &'static str {
        "Task"
    }

    fn borrows() -> Vec<ComponentBorrow> {
        let mut borrows = Vec::new();
        borrow_dyn_trait!(Self as AnyTask => borrows);
        borrows
    }
}

fn execute(world: &mut World) {
    let mut query = world.new_query().borrow_any::<&mut dyn AnyTask>();

    for task in query.iter_mut() {}
}
