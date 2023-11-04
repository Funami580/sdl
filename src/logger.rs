use chrono::Local;
use env_logger::fmt::{Color, Style, StyledValue};
use env_logger::{Builder, Logger};
use log::{Level, LevelFilter};

pub(crate) fn default_logger(debug: bool) -> Logger {
    formatted_local_time_builder("%H:%M:%S.%3f")
        .filter_level(if debug { LevelFilter::Trace } else { LevelFilter::Info })
        .parse_default_env()
        .build()
}

fn formatted_local_time_builder(fmt: &'static str) -> Builder {
    let mut builder = Builder::new();

    builder.format(|f, record| {
        use std::io::Write;

        let target = record.target();
        let crate_target = clap::crate_name!();

        if !(target == crate_target || target.starts_with(&format!("{crate_target}::"))) {
            return Ok(());
        }

        let mut style = f.style();
        let level = colored_level(&mut style, record.level());

        let time = Local::now().format(fmt);

        writeln!(f, "{} {} > {}", time, level, record.args())
    });

    builder
}

fn colored_level(style: &'_ mut Style, level: Level) -> StyledValue<'_, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}

/// Copy of indicatif_log_bridge with the ability to change the active
/// [MultiProgress].
pub(crate) mod log_wrapper {
    //! Tired of your log lines and progress bars mixing up?
    //! indicatif_log_bridge to the rescue!
    //!
    //! Simply wrap your favourite logging implementation in [LogWrapper]
    //!     and those worries are a thing of the past.
    //!
    //! Just remember to only use progress bars added to the [MultiProgress] you
    //! used     , otherwise you are back to ghostly halves of progress bars
    //! everywhere.
    //!
    //! # Example
    //! ```ignore
    //!     # use log::info;
    //!     # use indicatif::{MultiProgress, ProgressBar};
    //!     # use std::time::Duration;
    //!     let logger =
    //!         env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    //!             .build();
    //!     let multi = MultiProgress::new();
    //!
    //!     LogWrapper::new(multi.clone(), logger)
    //!         .try_init()
    //!         .unwrap();
    //!
    //!     let pg = multi.add(ProgressBar::new(10));
    //!     for i in (0..10) {
    //!         std::thread::sleep(Duration::from_micros(100));
    //!         info!("iteration {}", i);
    //!         pg.inc(1);
    //!     }
    //!     pg.finish();
    //!     multi.remove(&pg);
    //! ```
    //! The code of this crate is pretty simple, so feel free to check it out.

    use std::ops::Deref;
    use std::sync::{Arc, Mutex};

    use indicatif::MultiProgress;
    use log::Log;

    /// Wraps a MultiProgress and a Log implementor
    /// calling .suspend on the MultiProgress while writing the log message
    /// thereby preventing progress bars and logs from getting mixed up.
    ///
    /// You simply have to add all the progress bars in use to the MultiProgress
    /// in use.
    pub struct LogWrapper<L: Log> {
        bar: Arc<Mutex<Option<MultiProgress>>>,
        log: L,
    }

    impl<L: Log + 'static> LogWrapper<L> {
        pub fn new(bar: Option<MultiProgress>, log: L) -> Self {
            Self {
                bar: Arc::new(Mutex::new(bar)),
                log,
            }
        }

        /// Installs this as the lobal logger,
        ///
        /// tries to find the correct argument to set_max_level
        /// by reading the logger configuration,
        /// you may want to set it manually though.
        pub fn try_init(self) -> Result<SetLogWrapper, log::SetLoggerError> {
            use log::LevelFilter::*;
            let levels = [Off, Error, Warn, Info, Debug, Trace];

            for level_filter in levels.iter().rev() {
                let level = if let Some(level) = level_filter.to_level() {
                    level
                } else {
                    // off is the last level, just do nothing in that case
                    continue;
                };
                let meta = log::Metadata::builder().level(level).build();
                if self.enabled(&meta) {
                    log::set_max_level(*level_filter);
                    break;
                }
            }

            let cloned_bar = self.bar.clone();

            log::set_boxed_logger(Box::new(self)).map(|_| SetLogWrapper { bar: cloned_bar })
        }
    }

    pub struct SetLogWrapper {
        bar: Arc<Mutex<Option<MultiProgress>>>,
    }

    impl SetLogWrapper {
        pub fn set_multi(&mut self, multi: Option<MultiProgress>) {
            *self.bar.lock().unwrap() = multi;
        }
    }

    impl<L: Log> Log for LogWrapper<L> {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            self.log.enabled(metadata)
        }

        fn log(&self, record: &log::Record) {
            // do an early check for enabled to not cause unnescesary suspends
            if self.log.enabled(record.metadata()) {
                if let Some(bar) = self.bar.lock().unwrap().deref() {
                    bar.suspend(|| self.log.log(record));
                } else {
                    self.log.log(record);
                }
            }
        }

        fn flush(&self) {
            self.log.flush();
        }
    }
}
