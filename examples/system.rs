#![allow(dead_code)]

use edict::{system, ResNoSync, View};

struct A(*mut u8);

#[system]
fn foo(_a: View<&u32>, _b: ResNoSync<A>) {}

fn main() {}
