use core::{
    mem::{align_of, size_of, ManuallyDrop},
    ptr::drop_in_place,
};

use crate::world::World;

use super::ActionEncoder;

struct ActionFunPayload([usize; 3]);

struct ActionFunVTable {
    call: unsafe fn(&mut ActionFunPayload, &mut World, &mut ActionEncoder),
    drop: unsafe fn(&mut ActionFunPayload),
}

#[repr(C)]
struct ActionFunInline {
    payload: ActionFunPayload,
    vtable: &'static ActionFunVTable,
}

impl ActionFunInline {
    pub fn new<F>(fun: F) -> Result<Self, F>
    where
        F: FnOnce(&mut World, &mut ActionEncoder),
    {
        if size_of::<F>() > size_of::<ActionFunPayload>()
            || align_of::<F>() > align_of::<ActionFunPayload>()
        {
            return Err(fun);
        }

        let fun = ManuallyDrop::new(fun);
        let mut payload = [0usize; 3];

        unsafe {
            core::ptr::copy_nonoverlapping(
                &fun as *const _ as *const u8,
                payload.as_mut_ptr() as *mut u8,
                size_of::<F>(),
            );
        }

        Ok(ActionFunInline {
            payload: ActionFunPayload(payload),
            vtable: &ActionFunVTable {
                call: |payload, world, encoder| {
                    let fun = unsafe { core::ptr::read(payload as *mut _ as *mut F) };
                    fun(world, encoder);
                },
                drop: |payload| unsafe {
                    drop_in_place(payload as *mut _ as *mut F);
                },
            },
        })
    }
}

impl ActionFunInline {
    fn execute(self, world: &mut World, encoder: &mut ActionEncoder) {
        let mut me = ManuallyDrop::new(self);
        unsafe {
            (me.vtable.call)(&mut me.payload, world, encoder);
        }
    }
}

impl Drop for ActionFunInline {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            (self.vtable.drop)(&mut self.payload);
        }
    }
}

#[repr(C)]
struct ActionFunBoxed {
    fun: Box<dyn FnOnce(&mut World, &mut ActionEncoder)>,
    _pad: usize,
    zero: usize,
}

impl ActionFunBoxed {
    fn execute(self, world: &mut World, encoder: &mut ActionEncoder) {
        debug_assert_eq!(self.zero, 0);
        (self.fun)(world, encoder);
    }
}

pub union ActionFun {
    inlined: ManuallyDrop<ActionFunInline>,
    boxed: ManuallyDrop<ActionFunBoxed>,
    raw: [usize; 4],
}

impl ActionFun {
    #[inline]
    pub fn new<F>(fun: F) -> Self
    where
        F: FnOnce(&mut World, &mut ActionEncoder) + 'static,
    {
        match ActionFunInline::new(fun) {
            Ok(f) => ActionFun {
                inlined: ManuallyDrop::new(f),
            },
            Err(f) => ActionFun {
                boxed: ManuallyDrop::new(ActionFunBoxed {
                    fun: Box::new(f),
                    zero: 0,
                    _pad: 0,
                }),
            },
        }
    }

    #[inline]
    pub fn execute(self, world: &mut World, encoder: &mut ActionEncoder) {
        let mut me = ManuallyDrop::new(self);
        unsafe {
            if me.raw[3] != 0 {
                ManuallyDrop::take(&mut me.inlined).execute(world, encoder);
            } else {
                ManuallyDrop::take(&mut me.boxed).execute(world, encoder)
            }
        }
    }
}

impl Drop for ActionFun {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self.raw[3] != 0 {
                ManuallyDrop::drop(&mut self.inlined);
            } else {
                ManuallyDrop::drop(&mut self.boxed);
            }
        }
    }
}
