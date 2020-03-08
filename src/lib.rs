
//! #### &emsp;Chaotic testing harness
//!
//! **Kaos** is a chaotic testing harness to test your services against random failures.
//! It allows you to add points to your code to crash sporadically and
//! harness asserts availability and fault tolerance of your services by seeking
//! minimum time between failures, fail points, and randomized runs.
//!
//! Kaos is equivalent of Chaos Monkey for the Rust ecosystem. But it is more smart to find the closest MTBF based on previous runs.
//! This is dependable system practice. For more information please visit [Chaos engineering](https://en.wikipedia.org/wiki/Chaos_engineering).
//!
//! # Test Setup
//!
//! It is better to separate resilience tests.
//! Create a directory that will hold all chaos tests. In our example it will be `kaos-tests`.
//!
//! A minimal launcher for kaos setup looks like this:
//!
//! ```
//! #[test]
//! fn chaos_tests() {
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
//! name = "chaos_tests"
//! path = "kaos-tests/launcher.rs"
//! ```
//!
//! ## Definining flunks
//! In kaos there is a concept of [flunk]. Every flunk is a point of failure with panic. This can be redefinable.
//! After adding kaos as dependency you can add flunk points to define fallible operations or crucial points that system should continue its operation.
//!
//! Basic flunk is like:
//! ```rust
//! use kaos::flunk;
//! fn vec_check(v: &Vec<usize>) {
//!   if v.len() == 3 {
//!     flunk!("fail-when-three-elems");
//!   }
//! }
//! ```
//! This flunk point will be used later by kaos.
//!
//! ## Writing tests
//! Test harness will execute tests marked by a launcher. An example test for the flunk mentioned above is like this:
//! ```
//! # use std::panic;
//! # use kaos::flunk;
//! # fn vec_check(v: &Vec<usize>) {
//! #   if v.len() == 3 {
//! #     flunk!("fail-when-three-elems");
//! #   }
//! # }
//! use kaos::kaostest;
//!
//! kaostest!("fail-when-three-elems",
//!          {
//!              panic::catch_unwind(|| {
//!                let mut v = &mut vec![];
//!                loop {
//!                   v.push(1);
//!                   vec_check(v);
//!                }
//!              });
//!          }
//! );
//! ```
//! # Chaos Tests
//!
//! In addition to availability tests mentioned above we can test the software with chaos tests too.
//! For using chaotic measures and finding bare minimum failure, timing and MTBF combination
//! you can configure chaos tests in your launcher:
//!
//! ```
//! #[test]
//! fn chaos_tests() {
//!     let k = kaos::Runs::new();
//!
//!     for entry in fs::read_dir("kaos-tests").unwrap() {
//!         let entry = entry.unwrap();
//!         let path = entry.path();
//!
//!         // Let's have 10 varying runs.
//!         let run_count = 10;
//!
//!         // Minimum availability to expect as milliseconds for the runs.
//!         // Which corresponds as maximum surge between service runs.
//!         // Let's have it 10 seconds.
//!         let max_surge = 10 * 1000;
//!
//!         // Run chaotic test.
//!         k.chaotic(path, run_count, max_surge);
//!     }
//! }
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
pub use fail::FailScenario as KaosFailScenario;


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
