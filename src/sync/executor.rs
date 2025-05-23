extern crate alloc;
use super::action::Action;
use crate::{SplitTask, Task};
use alloc::sync::Arc;
use bumpalo::Bump;
use core::{mem::ManuallyDrop, pin::Pin};
use spin::Mutex;

pub struct Executor<A: Action> {
    #[allow(dead_code)]
    pub(crate) bump: Pin<Arc<Bump>>,
    pub(crate) action: A,
    pub(crate) task_ptrs: ManuallyDrop<Arc<[*const Task]>>, // use pointer to bypass borrow checker
    pub(crate) id: usize,
    pub(crate) min_chunk_size: u64,
    pub(crate) mutex: Arc<Mutex<()>>,
}

/// SAFETY:
/// we didn't allocate anything through bump where executing
unsafe impl<A: Action> Send for Executor<A> {}

/// SAFETY:
/// we didn't allocate anything through bump where executing
unsafe impl<A: Action> Sync for Executor<A> {}

impl<A: Action> Drop for Executor<A> {
    fn drop(&mut self) {
        let arc = unsafe { ManuallyDrop::take(&mut self.task_ptrs) };
        let count = Arc::strong_count(&arc);
        if count <= 1 {
            for task in arc.into_iter() {
                // drop all `Task`
                drop(unsafe { task.read() });
            }
        }
    }
}

impl<A: Action> Executor<A> {
    #[inline]
    pub fn run(&self) {
        let task = unsafe { self.task_ptrs[self.id].as_ref() }.unwrap();
        self.action.execute(self.id, task, &|| {
            let _guard = self.mutex.lock();
            let (max_pos, max_remain) = self
                .task_ptrs
                .iter()
                .enumerate()
                .map(|(i, w)| (i, unsafe { &**w }.remain()))
                .max_by_key(|(_, remain)| *remain)
                .unwrap();
            if max_remain < self.min_chunk_size {
                return false;
            }
            let (start, end) = unsafe { &*self.task_ptrs[max_pos] }.split_two();
            task.set_end(end);
            task.set_start(start);
            true
        })
    }
}
