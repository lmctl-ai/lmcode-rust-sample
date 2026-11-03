//! One-shot allocator rejection; no OS-wide memory exhaustion or policy quota.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use dslvm::Context;
struct Reject;
thread_local! { static UNTIL_FAILURE: Cell<isize> = const { Cell::new(-1) }; }
fn reject() -> bool { UNTIL_FAILURE.try_with(|counter| { let n=counter.get(); if n >= 0 { counter.set(n-1); } n==0 }).unwrap_or(false) }
unsafe impl GlobalAlloc for Reject {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if reject() { return std::ptr::null_mut(); }
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l); }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if reject() { return std::ptr::null_mut(); }
        System.realloc(p, l, n)
    }
}
#[global_allocator] static ALLOCATOR: Reject = Reject;
#[test]
fn arena_allocation_failures_are_structured_and_leave_no_live_object() {
    // Four bookkeeping buffers, type name and object storage, all independently fail.
    for fail in 0..6 {
        let mut context = Context::new();
        UNTIL_FAILURE.with(|counter| counter.set(fail));
        let result = context.insert_collection("IntList", Vec::<i64>::new());
        UNTIL_FAILURE.with(|counter| counter.set(-1));
        let error = result.expect_err("injected allocation must be reported");
        assert_eq!(error.code, "RUN_HOST");
        assert!(error.message.contains("allocation"));
        assert_eq!(context.len(), 0);
        context.insert_collection("IntList", Vec::<i64>::new()).unwrap();
    }
}
