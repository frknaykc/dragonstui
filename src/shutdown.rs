use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use signal_hook::{
    consts::signal::{SIGHUP, SIGINT, SIGTERM},
    flag,
};

/// Receives catchable process shutdown signals without doing terminal I/O in a signal handler.
#[derive(Clone, Debug)]
pub struct ShutdownSignal {
    requested: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn install() -> io::Result<Self> {
        let requested = Arc::new(AtomicBool::new(false));
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            flag::register(signal, Arc::clone(&requested))?;
        }
        Ok(Self { requested })
    }

    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
}
