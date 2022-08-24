//! Provides API to define task executors.

/// Abstract scoped task executor.
pub trait ScopedExecutor<'scope> {
    /// Spawns a task on the scope.
    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&Self) + Send + 'scope;
}

/// Mock executor that runs tasks on the current thread.
#[derive(Clone, Copy, Debug)]
pub struct MockExecutor;

impl<'scope> ScopedExecutor<'scope> for MockExecutor {
    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&Self) + Send + 'scope,
    {
        f(self)
    }
}

#[cfg(feature = "rayon")]
mod rayon {
    use super::ScopedExecutor;

    impl<'scope> ScopedExecutor<'scope> for rayon::Scope<'scope> {
        fn spawn<F>(&self, f: F)
        where
            F: FnOnce(&Self) + Send + 'scope,
        {
            self.spawn(f);
        }
    }
}

#[cfg(feature = "std")]
mod thread {
    use std::thread;

    use super::ScopedExecutor;

    impl<'scope> ScopedExecutor<'scope> for &'scope thread::Scope<'scope, '_> {
        fn spawn<F>(&self, f: F)
        where
            F: FnOnce(&Self) + Send + 'scope,
        {
            let scope = *self;
            scope.spawn(move || {
                f(&scope);
            });
        }
    }
}
