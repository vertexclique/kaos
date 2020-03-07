
//! #### &emsp;Chaotic testing harness
//!
//! Kaos is a chaotic testing test harness to test your services against random failures.
//! It allows you to add failpoints that panic randomly inside your code and randomizes
//! asserts availability and tolerance for resiliency for these faults.
//!
//! # Kaos Tests
//!
//! Create a directory that will hold all chaos tests called `kaos-tests`.
//!
//! A minimal launcher for kaos setup looks like this:
//!
//! ```
//! #[test]
//! fn kaos() {
//!     let k = kaos::Runs::new();
//!
//!     for entry in fs::read_dir("kaos-tests").unwrap() {
//!         let entry = entry.unwrap();
//!         let path = entry.path();
//!
//!         // Every service run should be available at least 2 seconds
//!         k.available(path, Duration::from_secs(2));
//!     }
//! }
//! ```
//!
//! and in your Cargo.toml
//!
//! ```toml
//! [[test]]
//! name = "kaos"
//! path = "kaos-tests/launcher.rs"
//! ```
//!
//! That's all, now what you have to do is run with `cargo test`.
//!
//! Kaos is using the same approach that [https://docs.rs/trybuild] has.
//! Instead of being compiler-like test harness, it has diverged to be chaos engineering
//! oriented harness.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/vertexclique/kaos/master/img/achaos.gif"
)]

extern crate humantime;

#[macro_use]
mod term;

#[macro_use]
mod path;

mod cargo;
mod dependencies;
mod diff;
mod env;
mod error;
mod features;
mod manifest;
mod message;
mod normalize;
mod run;
mod rustflags;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::{time::Duration, thread};


///
/// Chaotic runs test setup
#[derive(Debug)]
pub struct Runs {
    runner: RefCell<Runner>,
}

#[derive(Debug)]
struct Runner {
    tests: Vec<Test>,
}

#[derive(Clone, Debug)]
struct Test {
    path: PathBuf,
    duration: Option<Duration>,
    max_surge: isize,
    expected: Expected,
}

#[derive(Copy, Clone, Debug)]
enum Expected {
    Available,
    Chaotic
}

impl Runs {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Runs {
            runner: RefCell::new(Runner { tests: Vec::new() }),
        }
    }

    pub fn available<P: AsRef<Path>>(&self, path: P, duration: Duration) {
        self.runner.borrow_mut().tests.push(Test {
            path: path.as_ref().to_owned(),
            duration: Some(duration),
            max_surge: !0,
            expected: Expected::Available,
        });
    }

    pub fn chaotic<P: AsRef<Path>>(&self, path: P, run_count: usize, max_surge: usize) {
        (0..run_count).into_iter().for_each(|_| {
            self.runner.borrow_mut().tests.push(Test {
                path: path.as_ref().to_owned(),
                duration: None,
                max_surge: max_surge as isize,
                expected: Expected::Chaotic,
            });
        });
    }
}

#[doc(hidden)]
impl Drop for Runs {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.runner.borrow_mut().run();
        }
    }
}
