// Copyright 2019 Red Hat
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Frenetic is an implementation of stackful coroutines. It is written in Rust
//! and LLVM. Notably, this approach does not require any system calls or hand-
//! crafted assembly at all.
//!
//! # Example usage
//! ```
//! # #![cfg_attr(has_generator_trait, feature(generator_trait))]
//! use frenetic::{Coroutine, Generator, GeneratorState};
//! use core::pin::Pin;
//!
//! // You'll need to create a stack before using Frenetic coroutines.
//! let mut stack = [0u8; 4096 * 8];
//!
//! // Then, you can initialize with `Coroutine::new`.
//! let mut coro = Coroutine::new(&mut stack, |c| {
//!     let c = c.r#yield(1)?; // Yield an integer value.
//!     c.done("foo") // Return a string value.
//! });
//!
//! // You can also interact with the yielded and returned values.
//! match Pin::new(&mut coro).resume() {
//!     GeneratorState::Yielded(1) => {}
//!     _ => panic!("unexpected return from resume"),
//! }
//! match Pin::new(&mut coro).resume() {
//!     GeneratorState::Complete("foo") => {}
//!     _ => panic!("unexpected return from resume"),
//! }
//! ```

#![cfg_attr(has_generator_trait, feature(generator_trait))]
#![deny(
    warnings,
    absolute_paths_not_starting_with_crate,
    deprecated_in_future,
    keyword_idents,
    macro_use_extern_crate,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    unused_labels,
    unused_lifetimes,
    unreachable_pub,
    future_incompatible,
    missing_doc_code_examples,
    rust_2018_idioms,
    rust_2018_compatibility
)]

use core::ffi::c_void;
use core::mem::MaybeUninit;
#[cfg(has_generator_trait)]
pub use core::ops::{Generator, GeneratorState};
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

#[cfg(not(has_generator_trait))]
pub trait Generator {
    /// The type of value this generator yields.
    ///
    /// This associated type corresponds to the `yield` expression and the
    /// values which are allowed to be returned each time a generator yields.
    /// For example an iterator-as-a-generator would likely have this type as
    /// `T`, the type being iterated over.
    type Yield;

    /// The type of value this generator returns.
    ///
    /// This corresponds to the type returned from a generator either with a
    /// `return` statement or implicitly as the last expression of a generator
    /// literal. For example futures would use this as `Result<T, E>` as it
    /// represents a completed future.
    type Return;

    /// Resumes the execution of this generator.
    ///
    /// This function will resume execution of the generator or start execution
    /// if it hasn't already. This call will return back into the generator's
    /// last suspension point, resuming execution from the latest `yield`. The
    /// generator will continue executing until it either yields or returns, at
    /// which point this function will return.
    ///
    /// # Return value
    ///
    /// The `GeneratorState` enum returned from this function indicates what
    /// state the generator is in upon returning. If the `Yielded` variant is
    /// returned then the generator has reached a suspension point and a value
    /// has been yielded out. Generators in this state are available for
    /// resumption at a later point.
    ///
    /// If `Complete` is returned then the generator has completely finished
    /// with the value provided. It is invalid for the generator to be resumed
    /// again.
    ///
    /// # Panics
    ///
    /// This function may panic if it is called after the `Complete` variant has
    /// been returned previously. While generator literals in the language are
    /// guaranteed to panic on resuming after `Complete`, this is not guaranteed
    /// for all implementations of the `Generator` trait.
    fn resume(self: Pin<&mut Self>) -> GeneratorState<Self::Yield, Self::Return>;
}

#[cfg(not(has_generator_trait))]
pub enum GeneratorState<Y, R> {
    /// The generator suspended with a value.
    ///
    /// This state indicates that a generator has been suspended, and typically
    /// corresponds to a `yield` statement. The value provided in this variant
    /// corresponds to the expression passed to `yield` and allows generators to
    /// provide a value each time they yield.
    Yielded(Y),

    /// The generator completed with a return value.
    ///
    /// This state indicates that a generator has finished execution with the
    /// provided value. Once a generator has returned `Complete` it is
    /// considered a programmer error to call `resume` again.
    Complete(R),
}

pub struct Finished<R>(R);

pub struct Canceled(());

pub struct Coroutine<'a, Y, R>(Option<&'a mut Context<Y, R>>);

unsafe extern "C" fn callback<Y, R, F>(p: *mut *mut c_void, c: *mut c_void, f: *mut c_void) -> !
where
    F: FnOnce(Control<'_, Y, R>) -> Result<Finished<R>, Canceled>,
{
    // Allocate a Context and a closure.
    let mut ctx = MaybeUninit::zeroed().assume_init();
    let mut fnc = MaybeUninit::uninit().assume_init();

    // Cast the incoming pointers to their correct types.
    // See `Coroutine::new()`.
    let c = c as *mut Coroutine<'_, Y, R>;
    let f = f as *mut &mut F;

    // Pass references to the stack-allocated Context and closure back into
    // Coroutine::new() through the incoming pointers.
    (*c).0 = Some(&mut ctx);
    *f = &mut fnc;

    // Yield control to the parent. The first call to `Generator::resume()`
    // will resume at this location. The `Coroutine::new()` function is
    // responsible to move the closure into this stack while we are yielded.
    jump_swap(ctx.child.as_mut_ptr(), p);

    // Call the closure. If the closure returns, then move the return value
    // into the argument variable in `Generator::resume()`.
    if let Ok(r) = fnc(Control(&mut ctx)) {
        if !ctx.arg.is_null() {
            *ctx.arg = GeneratorState::Complete(r.0);
        }
    }

    // We cannot be resumed, so jump away forever.
    jump_into(ctx.parent.as_mut_ptr());
}

impl<'a, Y, R> Coroutine<'a, Y, R> {
    /// Spawns a new coroutine.
    ///
    /// This sets up the stack, and executes the closure within that stack.
    ///
    /// # Arguments
    ///
    /// * `stack` - A stack for this coroutine to use.
    /// This must be larger than `STACK_MINIMUM`, currently 4096, or Frenetic
    /// will panic.
    /// NOTE: It is up to the caller to properly allocate this stack. We
    /// recommend the stack include a guard page.
    ///
    /// * `func` - The closure to be executed as part of the coroutine.
    pub fn new<F>(stack: &'a mut [u8], func: F) -> Self
    where
        F: FnOnce(Control<'_, Y, R>) -> Result<Finished<R>, Canceled>,
    {
        // These variables are going to receive output from the callback
        // function above. Specifically, the callback function is going to
        // allocate space for a Context and our closure on the new stack. Then,
        // it is going to store references to those instances inside these
        // variables.
        let mut cor = Coroutine(None);
        let mut fnc: Option<&mut F> = None;

        assert!(stack.len() >= STACK_MINIMUM);

        unsafe {
            // Calculate the aligned top of the stack.
            let top = stack.as_mut_ptr().add(stack.len());
            let top = top.sub(top.align_offset(STACK_ALIGNMENT));

            // Call into the callback on the specified stack.
            jump_init(
                top,
                &mut cor as *mut _ as _,
                &mut fnc as *mut _ as _,
                callback::<Y, R, F>,
            );
        }

        // Move the closure onto the coroutine's stack.
        *fnc.unwrap() = func;

        cor
    }
}

pub struct Control<'a, Y, R>(&'a mut Context<Y, R>);

impl<'a, Y, R> Control<'a, Y, R> {
    /// Pauses execution of this coroutine, saves function position, and passes
    /// control back to parent.
    /// Returns a `Canceled` error if the parent has been dropped.
    ///
    /// # Arguments
    ///
    /// * `arg` - Passed on to the argument variable for the generator, if it
    /// exists.
    pub fn r#yield(self, arg: Y) -> Result<Self, Canceled> {
        unsafe {
            // The parent `Coroutine` object has been dropped. Resume the child
            // coroutine with the Canceled error. It must clean up and exit.
            if self.0.arg.is_null() {
                return Err(Canceled(()));
            }

            // Move the argument value into the argument variable in
            // `Generator::resume()`.
            *self.0.arg = GeneratorState::Yielded(arg);

            // Save our current position and yield control to the parent.
            jump_swap(self.0.child.as_mut_ptr(), self.0.parent.as_mut_ptr());

            // The parent `Coroutine` object has been dropped. Resume the child
            // coroutine with the Canceled error. It must clean up and exit.
            if self.0.arg.is_null() {
                return Err(Canceled(()));
            }
        }

        Ok(self)
    }

    /// Finishes execution of this coroutine.
    pub fn done<E>(self, arg: R) -> Result<Finished<R>, E> {
        Ok(Finished(arg))
    }
}

impl<'a, Y, R> Generator for Coroutine<'a, Y, R> {
    type Yield = Y;
    type Return = R;

    /// Resumes a paused coroutine.
    /// Re-initialize stack and continue execution where it was left off.
    fn resume(mut self: Pin<&mut Self>) -> GeneratorState<Y, R> {
        // Allocate an argument variable on the stack. See `Control::r#yield()` and
        // `callback()` for where this is initialized.
        let mut arg = unsafe { MaybeUninit::uninit().assume_init() };

        match self.0 {
            None => panic!("Called Generator::resume() after completion!"),
            Some(ref mut p) => unsafe {
                // Pass the pointer so that the child can move the argument out.
                p.arg = &mut arg;

                // Jump back into the child.
                jump_swap(p.parent.as_mut_ptr(), p.child.as_mut_ptr());

                // Clear the pointer as the value is about to become invalid.
                p.arg = null_mut();
            },
        }

        // If the child coroutine has completed, we are done. Make it so that
        // we can never resume the coroutine by clearing the reference.
        if let GeneratorState::Complete(r) = arg {
            self.0 = None;
            return GeneratorState::Complete(r);
        }

        arg
    }
}

impl<'a, Y, R> Drop for Coroutine<'a, Y, R> {
    fn drop(&mut self) {
        // If we are still able to resume the coroutine, do so. Since we don't
        // set the argument pointer, `Control::halt()` will return `Canceled`.
        if let Some(x) = self.0.take() {
            unsafe {
                jump_swap(x.parent.as_mut_ptr(), x.child.as_mut_ptr());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack() {
        let mut stack = [1u8; 4096 * 8];

        let mut coro = Coroutine::new(&mut stack, |c| {
            let c = c.r#yield(1)?;
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
        let mut stack = Box::new([1u8; 4096 * 8]);

        let mut coro = Coroutine::new(&mut *stack, |c| {
            let c = c.r#yield(1)?;
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
            let mut stack = [1u8; 4096 * 8];

            let mut coro = Coroutine::new(&mut stack, |c| match c.r#yield(1) {
                Ok(c) => c.done("foo"),
                Err(v) => {
                    cancelled = true;
                    Err(v)
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

    #[test]
    fn coro_early_drop_yield_done() {
        let mut stack = [1u8; 4096 * 8];

        let _coro = Coroutine::new(&mut stack, |c| {
            let c = c.r#yield(1)?;
            c.done("foo")
        });
    }

    #[test]
    fn coro_early_drop_done_only() {
        let mut stack = [1u8; 4096 * 8];

        let _coro = Coroutine::new(&mut stack, |c: Control<'_, i32, &str>| c.done("foo"));
    }

    #[test]
    fn coro_early_drop_result_ok() {
        let mut stack = [1u8; 4096 * 8];

        let _coro = Coroutine::new(&mut stack, |_c: Control<'_, i32, &str>| Ok(Finished("foo")));
    }

    #[test]
    fn coro_early_drop_result_err() {
        let mut stack = [1u8; 4096 * 8];

        let _coro = Coroutine::new(&mut stack, |_c: Control<'_, i32, &str>| Err(Canceled(())));
    }
}
