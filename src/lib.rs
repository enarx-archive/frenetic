#![cfg_attr(has_generator_trait, feature(generator_trait))]

use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::ptr::null_mut;

const STACK_ALIGNMENT: usize = 16;
const STACK_MINIMUM: usize = 4096;

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

pub struct Finished<R>(R);
pub struct Canceled(());

pub struct Coroutine<'a, Y, R>(Option<&'a mut Context<Y, R>>);

unsafe extern "C" fn callback<Y, R, F>(
    p: *mut *mut c_void,
    c: *mut c_void,
    f: *mut c_void,
) -> !
where
    F: FnOnce(Control<Y, R>) -> Result<Finished<R>, Canceled>,
{
    let mut ctx = MaybeUninit::uninit().assume_init();
    let mut fnc = MaybeUninit::uninit().assume_init();

    *(c as *mut Option<&mut Context<Y, R>>) = Some(&mut ctx);
    *(f as *mut &mut F) = &mut fnc;
    jump_swap(ctx.child.as_mut_ptr(), p);

    if let Ok(r) = fnc(Control(&mut ctx)) {
        *ctx.arg = GeneratorState::Complete(r.0);
    }

    jump_into(ctx.parent.as_mut_ptr());
}

impl<'a, Y, R> Coroutine<'a, Y, R> {
    pub fn new<F>(stack: &'a mut [u8], func: F) -> Self
    where
        F: FnOnce(Control<Y, R>) -> Result<Finished<R>, Canceled>,
    {
        let mut cor = Coroutine(None);
        let mut fnc: Option<&mut F> = None;

        assert!(stack.len() >= STACK_MINIMUM);

        unsafe {
            let top = stack.as_mut_ptr().add(stack.len());
            let top = top.sub(top.align_offset(STACK_ALIGNMENT));

            jump_init(
                top,
                &mut cor.0 as *mut _ as _,
                &mut fnc as *mut _ as _,
                callback::<Y, R, F>,
            );
        }

        *fnc.unwrap() = func;
        cor
    }
}

pub struct Control<'a, Y, R>(&'a mut Context<Y, R>);

impl<'a, Y, R> Control<'a, Y, R> {
    pub fn halt(self, arg: Y) -> Result<Self, Canceled> {
        unsafe {
            *self.0.arg = GeneratorState::Yielded(arg);

            jump_swap(self.0.child.as_mut_ptr(), self.0.parent.as_mut_ptr());

            if self.0.arg.is_null() {
                return Err(Canceled(()));
            }
        }

        Ok(self)
    }

    pub fn done<E>(self, arg: R) -> Result<Finished<R>, E> {
        Ok(Finished(arg))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack() {
        let mut stack = [0u8; 4096 * 8];

        let mut coro = Coroutine::new(&mut stack, |c| {
            let c = c.halt(1)?;
            c.done("foo")
        });

        match Pin::new(&mut coro).resume() {
            GeneratorState::Yielded(1) => {}
            _ => panic!("unexpected return from resume"),
        }

        match Pin::new(&mut coro).resume() {
            GeneratorState::Complete("foo") => {}
            _ => panic!("unexpected return from resume"),
        }
    }

    #[test]
    fn heap() {
        let mut stack = Box::new([0u8; 4096 * 8]);

        let mut coro = Coroutine::new(&mut *stack, |c| {
            let c = c.halt(1)?;
            c.done("foo")
        });

        match Pin::new(&mut coro).resume() {
            GeneratorState::Yielded(1) => {}
            _ => panic!("unexpected return from resume"),
        }

        match Pin::new(&mut coro).resume() {
            GeneratorState::Complete("foo") => {}
            _ => panic!("unexpected return from resume"),
        }
    }

    #[test]
    fn cancel() {
        let mut cancelled = false;

        {
            let mut stack = [0u8; 4096 * 8];

            let mut coro = Coroutine::new(&mut stack, |c| {
                match c.halt(1) {
                    Ok(c) => c.done("foo"),
                    Err(v) => {
                        cancelled = true;
                        Err(v)
                    },
                }
            });

            match Pin::new(&mut coro).resume() {
                GeneratorState::Yielded(1) => {}
                _ => panic!("unexpected return from resume"),
            }

            // Coroutine is cancelled when it goes out of scope.
        }

        assert!(cancelled);
    }
}
