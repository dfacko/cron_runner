use std::{
    error::Error,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex, OnceLock, RwLock,
    },
    thread::{sleep, Builder, JoinHandle},
    time::Duration,
};

use crate::{
    job::{IJob, Job},
    worker::Worker,
};

/// Runner type
/// Pooled - has n worker threads ready to do jobs and will not spawn any more
/// NonPooled - will spawn a worker thread every time a job is supposed to run
enum RunnerType {
    Pooled,
    NonPooled,
}
pub struct Runner {
    rtype: RunnerType,
    running: bool,
    handle: Option<JoinHandle<()>>,
    jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
    workers: Vec<Worker>,
    tx: Option<Sender<usize>>,
}

pub(crate) struct ScheduleTracker(Vec<usize>);

const RUNNER_LOOP_FREQUENCY: u64 = 500;

/// Tracker struct that will track scheduled jobs in a Pooled Runner
/// If for some reason all workers are busy, and a new job is scheduled
/// It will not be re-scheduled untill it was picked up and executed by a worker.
///
/// This is to avoid having one job scheduled n times because all workers were busy
/// and the job was supposed to be run (eg. You have 2 jobs and 1 worker, 1 takes 1 hour to finish, but the other one is scheduled
/// to run every minute, this way it will only be scheduled once)
impl ScheduleTracker {
    fn new() -> Self {
        Self(vec![])
    }

    fn scheduled(&self, job_id: usize) -> bool {
        self.0.contains(&job_id)
    }

    fn set_scheduled(&mut self, job_id: usize) {
        self.0.push(job_id);
    }

    pub fn unset_scheduled(&mut self, job_id: usize) {
        match self.0.iter().position(|id| id == &job_id) {
            Some(value) => self.0.remove(value),
            None => 0,
        };
    }
}

static TRACKER: OnceLock<Mutex<ScheduleTracker>> = OnceLock::new();

pub(crate) fn get_tracker() -> &'static Mutex<ScheduleTracker> {
    TRACKER.get_or_init(|| Mutex::new(ScheduleTracker::new()))
}

impl Runner {
    pub fn new(num_threads: usize) -> Self {
        match num_threads {
            n if n > 0 => {
                let (tx, rx) = mpsc::channel::<usize>();

                let rx = Arc::new(Mutex::new(rx));

                let jobs = Arc::new(RwLock::new(vec![]));
                let mut workers = Vec::with_capacity(num_threads);

                for id in 0..num_threads {
                    workers.push(Worker::new(id, Arc::clone(&rx), Arc::clone(&jobs)));
                }

                Runner {
                    rtype: RunnerType::Pooled,
                    running: false,
                    handle: None,
                    jobs,
                    workers,
                    tx: Some(tx),
                }
            }
            0 => Runner {
                rtype: RunnerType::NonPooled,
                running: false,
                handle: None,
                jobs: Arc::new(RwLock::new(vec![])),
                workers: vec![],
                tx: None,
            },
            _ => {
                panic!("Use either n > 0  workers or 0");
            }
        }
    }

    pub fn add_job(&mut self, job: Box<dyn Job>) {
        self.jobs.write().unwrap().push(Arc::new(RwLock::new(IJob {
            parallel_run_allowed: job.allow_parallel_runs(),
            job,
        })));
    }

    pub fn run(&mut self) {
        match self.rtype {
            RunnerType::Pooled => {
                for worker in self.workers.iter_mut() {
                    match Worker::spawn_pooled_worker_thread(
                        worker.worker_id,
                        worker.rx.clone(),
                        worker.jobs.clone(),
                    ) {
                        Ok(handle) => worker.handle = Some(handle),
                        Err(e) => eprintln!("Failed to spawn pooled worker thread with {:#?}", e),
                    }
                }

                match Self::spawn_pooled_runner_thread(self.jobs.clone(), self.tx.clone().unwrap())
                {
                    Ok(thread_handle) => {
                        self.handle = Some(thread_handle);
                        self.running = true;
                        println!("Pooled runner started");
                    }
                    Err(e) => {
                        eprint!("Failed to start pooled runner {:#?}", e);
                    }
                };
            }
            RunnerType::NonPooled => {
                match Self::spawn_non_pooled_runner_thread(self.jobs.clone()) {
                    Ok(thread_handle) => {
                        self.handle = Some(thread_handle);
                        self.running = true;
                        println!("Non pooled runner started");
                    }
                    Err(e) => {
                        eprint!("Failed to start non-pooled runner {:#?}", e);
                    }
                }
            }
        }
    }

    fn spawn_pooled_runner_thread(
        jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
        tx: Sender<usize>,
    ) -> Result<JoinHandle<()>, Box<dyn Error>> {
        match Builder::new().name("Runner".to_string()).spawn(move || {
            loop {
                for (id, job) in jobs.read().unwrap().iter().enumerate() {
                    let job_lock = job.try_read();

                    if let Ok(lock) = job_lock {
                        let mut tracker = get_tracker().lock().unwrap();

                        if lock.job.should_run() && !tracker.scheduled(id) {
                            let value = tx.send(id);

                            tracker.set_scheduled(id);
                            drop(tracker);

                            match value {
                                Ok(_) => (),
                                Err(e) => println!("Failed to schedule job with error {:#?}", e),
                            }
                        }
                    } else {
                        continue;
                    }
                }
                // We check twice a second if any job needs to run
                sleep(Duration::from_millis(RUNNER_LOOP_FREQUENCY));
            }
        }) {
            Ok(result) => Ok(result),
            Err(e) => Err(Box::new(e)),
        }
    }

    fn spawn_non_pooled_runner_thread(
        jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
    ) -> Result<JoinHandle<()>, Box<dyn Error>> {
        match Builder::new().name("Runner".to_string()).spawn(move || {
            loop {
                for (job_id, job_rw) in jobs.read().unwrap().iter().enumerate() {
                    let job_lock = job_rw.try_read();

                    if let Ok(job) = job_lock {
                        if job.job.should_run() {
                            match Worker::spawn_non_pooled_worker_thread(job_id, job_rw.clone()) {
                                Ok(_worker_handle) => (),
                                Err(e) => {
                                    eprintln!("Failed to spawn non pooled worker thread {}\n Trying to execute in local thread", e);
                                    
                                    job.job.execute();
                                }
                            }
                        }
                    } else {
                        continue;
                    }
                }
                // We check twice a second if any job needs to run
                sleep(Duration::from_millis(RUNNER_LOOP_FREQUENCY));
            }
        }) {
            Ok(result) => Ok(result),
            Err(e) => Err(Box::new(e)),
        }
    }
}
