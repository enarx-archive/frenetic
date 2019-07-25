[![Build Status](https://travis-ci.org/enarx/frenetic.svg?branch=master)](https://travis-ci.org/enarx/frenetic)
![Rust Version 1.36+](https://img.shields.io/badge/rustc-v1.36%2B-blue.svg)
[![Crate](https://img.shields.io/crates/v/frenetic.svg)](https://crates.io/crates/frenetic)
[![Docs](https://docs.rs/frenetic/badge.svg)](https://docs.rs/frenetic)
![License](https://img.shields.io/crates/l/frenetic.svg?style=popout)

# Frenetic

Frenetic is an implementation of stackful coroutines. It is
written in Rust and LLVM. Notably, this approach does not require any system
calls or hand-crafted assembly at all.

## Examples

```
use frenetic::{Coroutine, Generator, GeneratorState};
use core::pin::Pin;

// You'll need to create a stack before using Frenetic coroutines.
let mut stack = [0u8; 4096 * 8];

// Then, you can initialize with `Coroutine::new`.
let mut coro = Coroutine::new(&mut stack, |c| {
    let c = c.r#yield(1)?; // Yield an integer value.
    c.done("foo") // Return a string value.
});

// You can also interact with the yielded and returned values.
match Pin::new(&mut coro).resume() {
    GeneratorState::Yielded(1) => {}
    _ => panic!("unexpected return from resume"),
}
match Pin::new(&mut coro).resume() {
    GeneratorState::Complete("foo") => {}
    _ => panic!("unexpected return from resume"),
}
```

That's it!

## API

The current API consists of a few basic primitives:

### `Coroutine::new`
Spawns a new coroutine. Requires a stack and a function to be executed.

*NOTE:* The caller is responsible for properly allocating this stack. We recommend the stack includes a guard page.

### `Control::r#yield`
Halts the current coroutine's execution and passes control back to the parent.

### `Control::done`
Marks the current coroutine as done, and finishes.

### `Generator::resume`
Resumes a halted coroutine.