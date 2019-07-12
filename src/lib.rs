#![feature(generator_trait)]

extern "C" {
    fn stack_get() -> *mut i8;
    fn stack_set(ptr: *mut i8);
}

struct Context;
