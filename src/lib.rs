#![cfg_attr(has_generator_trait, feature(generator_trait))]

use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::ptr::null_mut;

extern "C" {
    fn jump_into(into: *mut *mut c_void) -> !;
    fn jump_swap(from: *mut *mut c_void, into: *mut *mut c_void);
    fn jump_init(
        stack: *mut u8,
        ctx: *mut c_void,
        fnc: *mut c_void,
        func: unsafe extern "C" fn(
            parent: *mut *mut c_void,
            ctxpp: *mut c_void,
            fncpp: *mut c_void,
        ) -> !,
    );
}

struct Context<Y, R> {
    parent: [*mut c_void; 5],
    child: [*mut c_void; 5],
    arg: *mut GeneratorState<Y, R>,
}

#[cfg(has_generator_trait)]
pub use core::ops::{Generator, GeneratorState};

#[cfg(not(has_generator_trait))]
pub trait Generator {
    type Yield;
    type Return;
    fn resume(
        self: Pin<&mut Self>,
    ) -> GeneratorState<Self::Yield, Self::Return>;
}

#[cfg(not(has_generator_trait))]
pub enum GeneratorState<Y, R> {
    Yielded(Y),
    Complete(R),
}

pub struct Coroutine<'a, Y, R>(Option<&'a mut Context<Y, R>>, &'a mut [u8]);
pub struct Control<'a, Y, R>(&'a mut Context<Y, R>);
pub struct Cancelled(());

impl<'a, Y, R> Coroutine<'a, Y, R> {
    pub fn new<F>(stack: &'a mut [u8], func: F) -> Self
    where
        F: FnOnce(Control<Y, R>) -> R,
    {
        let mut ctx: Option<&mut Context<Y, R>> = None;
        let mut fnc: Option<&mut F> = None;

        unsafe {
            jump_init(
                stack.as_mut_ptr().add(stack.len()), // Top of the stack.
                &mut ctx as *mut _ as _,
                &mut fnc as *mut _ as _,
                callback::<Y, R, F>,
            );

            *fnc.unwrap() = func;

            return Coroutine(ctx, stack);
        }

        extern "C" fn callback<Y, R, F>(
            p: *mut *mut c_void,
            c: *mut c_void,
            f: *mut c_void,
        ) -> !
        where
            F: FnOnce(Control<Y, R>) -> R,
        {
            unsafe {
                let mut ctx = MaybeUninit::uninit().assume_init();
                let mut fnc = MaybeUninit::uninit().assume_init();

                *(c as *mut &mut Context<Y, R>) = &mut ctx;
                *(f as *mut &mut F) = &mut fnc;
                jump_swap(ctx.child.as_mut_ptr(), p);

                let r = fnc(Control(&mut ctx));
                *ctx.arg = GeneratorState::Complete(r);
                jump_into(ctx.parent.as_mut_ptr());
            }
        }
    }
}

impl<'a, Y, R> Control<'a, Y, R> {
    pub fn pause(self, arg: Y) -> Result<Self, Cancelled> {
        unsafe {
            *self.0.arg = GeneratorState::Yielded(arg);

            jump_swap(self.0.child.as_mut_ptr(), self.0.parent.as_mut_ptr());

            if self.0.arg.is_null() {
                return Err(Cancelled(()));
            }
        }

        Ok(self)
    }
}

impl<'a, Y, R> Generator for Coroutine<'a, Y, R> {
    type Yield = Y;
    type Return = R;

    fn resume(mut self: Pin<&mut Self>) -> GeneratorState<Y, R> {
        let mut arg = unsafe { MaybeUninit::uninit().assume_init() };

        match self.0 {
            None => panic!("Called Generator::resume() after completion!"),
            Some(ref mut p) => unsafe {
                p.arg = &mut arg;
                jump_swap(p.parent.as_mut_ptr(), p.child.as_mut_ptr());
                p.arg = null_mut();
            },
        }

        if let GeneratorState::Complete(r) = arg {
            self.0 = None;
            return GeneratorState::Complete(r);
        }

        arg
    }
}

impl<'a, Y, R> Drop for Coroutine<'a, Y, R> {
    fn drop(&mut self) {
        if let Some(ref mut x) = self.0 {
            unsafe { jump_swap(x.parent.as_mut_ptr(), x.child.as_mut_ptr()); }
            self.0 = None;
        }
    }
}
