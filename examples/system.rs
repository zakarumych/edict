#![allow(dead_code)]

use edict::{
    system::{system, ResLocal},
    view::View,
};

struct A(*mut u8);

#[system]
fn foo(_a: View<&u32>, _b: ResLocal<A>) {}

fn main() {}
