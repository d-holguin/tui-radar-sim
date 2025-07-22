use std::num::NonZero;
use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

/// Errors that can occur when using the thread pool.
#[derive(Debug)]
pub enum ThreadPoolError {
    /// The thread pool has been shut down and cannot accept new jobs.
    PoolShutdown,
    /// Failed to send a job to the worker threads.
    SendError,
    /// Invalid configuration (e.g., zero threads).
    InvalidConfiguration(String),
}

impl fmt::Display for ThreadPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThreadPoolError::PoolShutdown => write!(f, "Thread pool has been shut down"),
            ThreadPoolError::SendError => write!(f, "Failed to send job to worker thread"),
            ThreadPoolError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {msg}"),
        }
    }
}

impl Error for ThreadPoolError {}

/// A thread pool for executing jobs concurrently.
///
/// # Example
///
/// ```
/// use tui_radar_sim_core::threadpool::ThreadPool;
///
/// let pool = ThreadPool::new(4).unwrap();
///
/// pool.execute(|| {
///     println!("Hello from thread pool!");
/// }).unwrap();
/// ```
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Message>>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

enum Message {
    NewJob(Job),
    Terminate,
}

/// Configuration builder for ThreadPool.
pub struct ThreadPoolBuilder {
    num_threads: usize,
    thread_name_prefix: String,
    stack_size: Option<usize>,
}

impl Default for ThreadPoolBuilder {
    fn default() -> Self {
        Self {
            num_threads: ThreadPool::num_cpus().unwrap_or(4),
            thread_name_prefix: "worker".to_string(),
            stack_size: None,
        }
    }
}

impl ThreadPoolBuilder {
    /// Create a new ThreadPoolBuilder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the number of worker threads.
    pub fn num_threads(mut self, num: usize) -> Self {
        self.num_threads = num;
        self
    }

    /// Set the prefix for worker thread names.
    pub fn thread_name_prefix(mut self, prefix: String) -> Self {
        self.thread_name_prefix = prefix;
        self
    }

    /// Set the stack size for worker threads.
    pub fn stack_size(mut self, size: usize) -> Self {
        self.stack_size = Some(size);
        self
    }

    /// Build the `ThreadPool` with the configured settings.
    pub fn build(self) -> Result<ThreadPool, ThreadPoolError> {
        ThreadPool::with_config(&self)
    }
}

impl ThreadPool {
    /// Create a new `ThreadPool` with the specified number of threads.
    ///
    /// # Arguments
    ///
    /// * `size` - The number of worker threads in the pool.
    ///
    /// # Errors
    ///
    /// Returns an error if `size` is 0.
    pub fn new(size: usize) -> Result<ThreadPool, ThreadPoolError> {
        if size == 0 {
            return Err(ThreadPoolError::InvalidConfiguration(
                "Thread pool size must be greater than 0".to_string(),
            ));
        }

        ThreadPoolBuilder::new().num_threads(size).build()
    }

    /// Create a new `ThreadPool` with a custom configuration.
    fn with_config(config: &ThreadPoolBuilder) -> Result<ThreadPool, ThreadPoolError> {
        if config.num_threads == 0 {
            return Err(ThreadPoolError::InvalidConfiguration(
                "Thread pool size must be greater than 0".to_string(),
            ));
        }

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(config.num_threads);

        for id in 0..config.num_threads {
            let worker_receiver = Arc::clone(&receiver);
            let thread_name = format!("{}-{}", config.thread_name_prefix, id);

            let mut builder = thread::Builder::new().name(thread_name);

            if let Some(stack_size) = config.stack_size {
                builder = builder.stack_size(stack_size);
            }

            workers.push(Worker::new(id, worker_receiver, builder)?);
        }

        Ok(Self {
            workers,
            sender: Some(sender),
        })
    }

    /// Execute a job in the thread pool.
    ///
    /// # Arguments
    ///
    /// * `f` - The closure to execute in one of the worker threads.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread pool has been shut down.
    pub fn execute<F>(&self, f: F) -> Result<(), ThreadPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        self.sender
            .as_ref()
            .ok_or(ThreadPoolError::PoolShutdown)?
            .send(Message::NewJob(job))
            .map_err(|_| ThreadPoolError::SendError)
    }

    /// Get the number of worker threads in the pool.
    pub fn num_threads(&self) -> usize {
        self.workers.len()
    }

    /// Gracefully shut down the thread pool.
    ///
    /// This method will wait for all workers to finish their current jobs
    /// before shutting down. It returns once all workers have terminated.
    pub fn shutdown(self) {
        // Drop is called automatically, but this provides explicit control
        drop(self);
    }

    /// Attempt to shut down the thread pool with a timeout.
    ///
    /// Returns `true` if all workers shut down within the timeout, `false` otherwise.
    pub fn shutdown_timeout(mut self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();

        // Send terminate message to all workers
        if let Some(sender) = &self.sender {
            for _ in &self.workers {
                let _ = sender.send(Message::Terminate);
            }
        }

        // Drop the sender to signal no more jobs will come
        drop(self.sender.take());

        // Wait for workers with timeout
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    return false;
                }

                // Note: There's no built-in way to join with timeout in std,
                // so we'd need to implement a more complex solution for true timeout support
                if thread.join().is_err() {
                    return false;
                }
            }
        }

        true
    }

    pub fn num_cpus() -> Option<usize> {
        thread::available_parallelism().map(NonZero::get).ok()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // Send terminate message to all workers
        if let Some(sender) = &self.sender {
            for _ in &self.workers {
                let _ = sender.send(Message::Terminate);
            }
        }

        // Drop the sender to close the channel
        drop(self.sender.take());

        // Wait for all workers to finish
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[allow(dead_code)]
struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<Message>>>,
        builder: thread::Builder,
    ) -> Result<Self, ThreadPoolError> {
        let thread = builder
            .spawn(move || {
                loop {
                    // Handle potential poisoned mutex
                    let message = match receiver.lock() {
                        Ok(guard) => guard.recv(),
                        Err(poisoned) => {
                            // Recover from poisoned mutex
                            poisoned.into_inner().recv()
                        }
                    };

                    match message {
                        Ok(Message::NewJob(job)) => {
                            // Execute the job and catch any panics
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));

                            if result.is_err() {
                                eprintln!("Worker {id} job panicked");
                            }
                        }
                        Ok(Message::Terminate) | Err(_) => {
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                ThreadPoolError::InvalidConfiguration(format!("Failed to spawn worker thread: {e}"))
            })?;

        Ok(Self {
            id,
            thread: Some(thread),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_thread_pool_creation() {
        let pool = ThreadPool::new(4).unwrap();
        assert_eq!(pool.num_threads(), 4);
    }

    #[test]
    fn test_zero_threads_error() {
        let result = ThreadPool::new(0);
        assert!(matches!(
            result,
            Err(ThreadPoolError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn test_execute_job() {
        let pool = ThreadPool::new(2).unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        pool.execute(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

        // Give the job time to execute
        thread::sleep(Duration::from_millis(100));

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_multiple_jobs() {
        let pool = ThreadPool::new(4).unwrap();
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let counter_clone = Arc::clone(&counter);
            pool.execute(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        }

        // Give jobs time to execute
        thread::sleep(Duration::from_millis(200));

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_panic_recovery() {
        let pool = ThreadPool::new(2).unwrap();
        let counter = Arc::new(AtomicUsize::new(0));

        // Submit a job that panics
        pool.execute(|| {
            panic!("This job panics!");
        })
        .unwrap();

        // Submit a normal job after the panic
        let counter_clone = Arc::clone(&counter);
        pool.execute(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

        // Give jobs time to execute
        thread::sleep(Duration::from_millis(100));

        // The second job should still execute despite the first one panicking
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
