#![cfg_attr(has_generator_trait, feature(generator_trait))]

use frenetic::{Coroutine, Generator, GeneratorState};
use std::pin::Pin;

#[test]
fn heap() {
    let mut stack = Box::new([0u8; 4096 * 8]);

    unsafe {
        eprintln!("stack bot: 0x{:p}", stack.as_mut_ptr());
        eprintln!("stack top: 0x{:p}", stack.as_mut_ptr().add(stack.len()));
    }

    let mut coro = Coroutine::new(&mut *stack, |c| {
        eprintln!("started");
        let c = c.pause(1)?;
        eprintln!("resumed");
        let _ = c.pause(2)?;
        eprintln!("resumed");
        Ok("foo")
    });

    match Pin::new(&mut coro).resume() {
        GeneratorState::Yielded(1) => {}
        _ => panic!("unexpected return from resume"),
    }

    match Pin::new(&mut coro).resume() {
        GeneratorState::Yielded(2) => {}
        _ => panic!("unexpected return from resume"),
    }

    match Pin::new(&mut coro).resume() {
        GeneratorState::Complete("foo") => {}
        _ => panic!("unexpected return from resume"),
    }
}
