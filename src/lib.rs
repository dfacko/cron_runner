//! # cron_runner: a cron job runner
//!
//! Use the Job trait to define Jobs on your structs, pass the jobs to the Runner and start it with run().
//! The Runner will spawn a main "monitoring" thread to schedule your jobs, and additional worker threads to run your jobs.
//!
//! ## The Runner can run in 2 modes.
//!
//! ##### Pooled
//! Spawn it with n > 0 threads and those threads will live in a thread pool waiting for new jobs from a mpsc channel ( in this context used like a FIFO queue).
//! While the worker threads are waiting, they are blocked on recv().
//! If all workers are busy new jobs will not be run untill a worker is free, the first job to be scheduled is the first to be picked up and run by a worker.
//!
//! ##### Non Pooled
//! If you start the runner with 0 worker threads, any time a job should run a new thread will be spawned for the job that will execute it and return.
//! If for some reason a worker could not be spawned, the Runner will try to run the job in its main thread, blocking scheduling for the rest of the jobs untill it is done.
//!
//! ## Example
//! ```
//! use cron_runner::{Job, Runner, Schedule};
//!
//! struct PrintJob {
//!     message: String,
//! }
//!
//! impl Job for PrintJob {
//!     fn execute(&self) {
//!         println!("{}", self.message);
//!     }
//!
//!     fn schedule(&self) -> Schedule {
//!         "0 * * * * *".parse().unwrap()
//!     }
//! }
//!
//! fn main() {
//!     let mut runner = Runner::new(2);
//!
//!     runner.add_job(Box::new(PrintJob {
//!         message: "Print job".to_string(),
//!     }));
//!
//!     runner.run();
//!
//!     loop {
//!         std::thread::sleep(std::time::Duration::from_secs(5));
//!     }
//! }
//! ```
//! ```
//! Output:
//! Worker started job 0 at 15:12:59.632829567
//! Print job
//! Worker finished job 0 at 15:12:59.632854926
//!
//!

mod job;
mod runner;
mod worker;

pub use job::{Job, Schedule};
pub(crate) use runner::get_tracker;
pub use runner::Runner;
