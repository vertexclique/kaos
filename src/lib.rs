
//! #### &emsp;Chaotic testing harness
//!
//! Kaos is a chaotic testing harness to test your services against random failures.
//! It allows you to add points to your code to crash sporadically and
//! harness asserts availability and fault tolerance of your services by seeking
//! minimum time between failures, fail points, and randomized runs.
//!
//! Kaos is equivalent of Chaos Monkey for the Rust ecosystem. But it is more smart to find the closest MTBF based on previous runs.
//! This is dependable system practice. For more information please visit [Chaos engineering](https://en.wikipedia.org/wiki/Chaos_engineering).
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
//! Kaos is using the same approach that [trybuild](https://docs.rs/trybuild) has.
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
mod macros;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::{time::Duration, thread};

#[doc(hidden)]
pub use fail::eval as flunker;
#[doc(hidden)]
pub use fail::cfg as flunker_cfg;
#[doc(hidden)]
pub use fail::FailScenario as Scene;


pub use macros::*;

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
