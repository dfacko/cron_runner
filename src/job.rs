use chrono::{DateTime, TimeDelta, Utc};
pub use cron::Schedule;

const ALLOWED_JOB_TIME_DELTA: i64 = 500;

/// Job Trait
///
/// Implement this trait for any struct you want to run as a Job.
///
///
pub trait Job: Send + Sync {
    fn execute(&self);

    /// Returns whether the job should run now or not
    fn should_run(&self) -> bool {
        for item in self.schedule().upcoming(Utc).take(1) {
            let now = Utc::now();
            let difference = item - now;
            if difference <= TimeDelta::milliseconds(ALLOWED_JOB_TIME_DELTA) {
                return true;
            }
        }

        false
    }

    /// Define a schedule for your job
    fn schedule(&self) -> Schedule;

    /// If you want your Job to run again, while another instance of it is already running overwrite this
    /// The default behaviour is that while a job is running it will not be executed again.
    fn allow_parallel_runs(&self) -> bool {
        false
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Temporary struct to hold a job
/// Currently there is no need for it and can be removed
#[allow(dead_code)]
pub(crate) struct IJob {
    pub job: Box<dyn Job>,
    pub parallel_run_allowed: bool,
}
