use std::{
    error::Error,
    sync::{Arc, Mutex, RwLock},
    thread::{Builder, JoinHandle},
};

use chrono::Utc;

use crate::job::IJob;

pub struct Worker {
    pub handle: Option<std::thread::JoinHandle<()>>,
    pub worker_id: usize,
    pub rx: Arc<Mutex<std::sync::mpsc::Receiver<usize>>>,
    pub jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
}

impl Worker {
    pub fn new(
        worker_id: usize,
        rx: Arc<Mutex<std::sync::mpsc::Receiver<usize>>>,
        jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
    ) -> Self {
        Self {
            handle: None,
            worker_id,
            jobs,
            rx,
        }
    }

    pub fn spawn_pooled_worker_thread(
        worker_id: usize,
        rx: Arc<Mutex<std::sync::mpsc::Receiver<usize>>>,
        jobs: Arc<RwLock<Vec<Arc<RwLock<IJob>>>>>,
    ) -> Result<JoinHandle<()>, Box<dyn Error>> {
        match Builder::new().spawn(move || loop {
            let rx_guard = rx.lock().unwrap();

            let job_id = rx_guard.recv();

            drop(rx_guard);

            match job_id {
                Ok(id) => {
                    println!("Worker {} recieved job {}", worker_id, id);

                    let job_vec_rw_lock = jobs.read().unwrap();
                    let job = job_vec_rw_lock[id].clone();
                    let mut job_write_lock = None;
                    let mut job_read_lock = None;

                    Self::lock_job(&job, &mut job_write_lock, &mut job_read_lock);

                    drop(job_vec_rw_lock);

                    //Remove job_id from tracker so it can be scheduled again if parallel runs are allowed
                    let mut tracker = crate::get_tracker().lock().unwrap();
                    tracker.unset_scheduled(id);
                    drop(tracker);

                    if let Some(lock) = job_write_lock {
                        println!(
                            "Worker {} started job {} at {}",
                            worker_id,
                            id,
                            Utc::now().format("%H:%M:%S%.f")
                        );

                        lock.job.execute();

                        println!(
                            "Worker {} finished job {} at {}",
                            worker_id,
                            id,
                            Utc::now().format("%H:%M:%S%.f")
                        );
                    } else {
                        let lock = job_read_lock.unwrap();
                        println!(
                            "Worker {} started job {} at {}",
                            worker_id,
                            id,
                            Utc::now().format("%H:%M:%S%.f")
                        );

                        lock.job.execute();

                        println!(
                            "Worker {} finished job {} at {}",
                            worker_id,
                            id,
                            Utc::now().format("%H:%M:%S%.f")
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Worker {} failed to receieve with error: {:#?}",
                        worker_id, e
                    );
                }
            }
        }) {
            Ok(handle) => {
                println!("Worker {} ready.", worker_id);
                Ok(handle)
            }
            Err(e) => Err(Box::new(e)),
        }
    }

    #[allow(clippy::blocks_in_conditions)]
    pub fn spawn_non_pooled_worker_thread(
        job_id: usize,
        job: Arc<RwLock<IJob>>,
    ) -> Result<JoinHandle<()>, Box<dyn Error>> {
        match Builder::new().spawn(move || {
            let mut job_write_lock = None;
            let mut job_read_lock = None;

            Self::lock_job(&job, &mut job_write_lock, &mut job_read_lock);

            if let Some(lock) = job_write_lock {
                println!(
                    "Worker started job {} at {}",
                    job_id,
                    Utc::now().format("%H:%M:%S%.f")
                );

                lock.job.execute();

                println!(
                    "Worker finished job {} at {}",
                    job_id,
                    Utc::now().format("%H:%M:%S%.f")
                );
            } else {
                let lock = job_read_lock.unwrap();
                println!(
                    "Worker started job {} at {}",
                    job_id,
                    Utc::now().format("%H:%M:%S%.f")
                );

                lock.job.execute();

                println!(
                    "Worker finished job {} at {}",
                    job_id,
                    Utc::now().format("%H:%M:%S%.f")
                );
            }
        }) {
            Ok(handle) => Ok(handle),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Helper function to lock a job from the vector of jobs
    /// If parallel runs are allowed the lock will be a read lock
    /// If parallel runs are not allowed(default) it will be a write lock
    fn lock_job<'a>(
        job: &'a Arc<RwLock<IJob>>,
        job_write_lock: &mut Option<std::sync::RwLockWriteGuard<'a, IJob>>,
        job_read_lock: &mut Option<std::sync::RwLockReadGuard<'a, IJob>>,
    ) {
        let job_write_lock_result = job.try_write();
        let job_read_lock_result;

        match job_write_lock_result {
            Ok(write_lock) => {
                *job_write_lock = Some(write_lock); // move the value out of this scope
                if !job_write_lock.as_ref().unwrap().parallel_run_allowed {
                    // if parallel runs are not allowed, we keep the write lock an return the job
                    //&job_write_lock.job
                } else {
                    // if parallel runs are allowed, we instead acquire a read lock and return the job
                    //drop(job_write_lock.unwrap());
                    *job_write_lock = None;
                    *job_read_lock = Some(job.read().unwrap());
                    //&job_read_lock.job
                }
            }
            Err(_) => {
                // We could not get a write lock (another lock already exists)
                job_read_lock_result = job.try_read();

                match job_read_lock_result {
                    Ok(read_lock) => {
                        // Here we know another lock already exists, and we know it must be a read lock beacuse we acquired another read lock
                        // No need to check if paralel run is allowed, the only way to get a read lock and not a write lock if it was allowed in the first place

                        *job_read_lock = Some(read_lock); // move the value out of match scope

                        //&job_read_lock.job
                    }
                    Err(_) => {
                        // In this case we could not get a write or read lock on the Job, this should never happen
                        // and if it does idk :D
                        todo!();
                    }
                }
            }
        }
    }
}
