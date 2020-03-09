use std::collections::BTreeMap as Map;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{Expected, Runner, Test};
use crate::cargo;
use crate::dependencies::{self, Dependency};
use crate::env::Update;
use crate::error::{Error, Result};
use crate::features;
use crate::manifest::{Bin, Build, Config, Manifest, Name, Package, Workspace};
use crate::message::{self, Fail, Warn};
use crate::normalize::{self, Context, Variations};
use crate::rustflags;
use humantime::format_duration;
use proptest::test_runner::{TestCaseError, TestRunner};
use std::convert::TryInto;

#[derive(Debug)]
pub struct Project {
    pub dir: PathBuf,
    source_dir: PathBuf,
    pub target_dir: PathBuf,
    pub name: String,
    update: Update,
    pub has_run_at_least: bool,
    pub surges: Vec<isize>,
    test_idx: usize,
    pub durations: Vec<Option<Duration>>,
    has_compile_fail: bool,
    pub features: Option<Vec<String>>,
    workspace: PathBuf,
}

impl Runner {
    pub fn run(&mut self) {
        let mut tests = expand_globs(&self.tests);
        filter(&mut tests);

        let mut project = self.prepare(&tests).unwrap_or_else(|err| {
            message::prepare_fail(err);
            panic!("tests failed");
        });

        print!("\n\n");

        let len = tests.len();
        let mut failures = 0;

        if tests.is_empty() {
            message::no_tests_enabled();
        } else {
            for test in tests {
                if let Err(err) = test.run(&mut project) {
                    failures += 1;
                    message::test_fail(err);
                }
            }
        }

        print!("\n\n");

        if failures > 0 && project.name != "kaos-tests" {
            panic!("{} of {} tests failed", failures, len);
        }
    }

    fn prepare(&self, tests: &[ExpandedTest]) -> Result<Project> {
        let metadata = cargo::metadata()?;
        let target_dir = metadata.target_directory;
        let workspace = metadata.workspace_root;

        let crate_name = env::var("CARGO_PKG_NAME").map_err(Error::PkgName)?;

        let mut has_run_at_least = false;
        let mut has_compile_fail = false;
        for e in tests {
            match e.test.expected {
                Expected::Available => has_run_at_least = true,
                Expected::Chaotic => has_compile_fail = true,
            }
        }

        let surges: Vec<isize> = tests.iter().map(|t| t.test.max_surge).collect();

        let mut static_durations: Vec<Option<Duration>> = vec![None; surges.len()];

        surges
            .iter()
            .position(|&e| e == !0)
            .map(|i| static_durations[i] = tests[i].test.duration);

        let source_dir = env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .ok_or(Error::ProjectDir)?;

        let features = features::find();

        let mut project = Project {
            dir: path!(target_dir / "tests" / crate_name),
            source_dir,
            target_dir,
            name: format!("{}-tests", crate_name),
            update: Update::env()?,
            surges,
            test_idx: 0,
            durations: static_durations,
            has_run_at_least,
            has_compile_fail,
            features,
            workspace,
        };

        let manifest = self.make_manifest(crate_name, &project, tests)?;
        let manifest_toml = toml::to_string(&manifest)?;

        let config = self.make_config();
        let config_toml = toml::to_string(&config)?;

        match &mut project.features {
            Some(enabled_features) => {
                enabled_features.retain(|feature| manifest.features.contains_key(feature));
                // enabled_features.push("fail/failpoints".into());
            }
            _ => {
                // project.features = Some(vec!["fail/failpoints".into()]);
            }
        }

        fs::create_dir_all(path!(project.dir / ".cargo"))?;
        fs::write(path!(project.dir / ".cargo" / "config"), config_toml)?;
        fs::write(path!(project.dir / "Cargo.toml"), manifest_toml)?;
        fs::write(path!(project.dir / "main.rs"), b"fn main() {}\n")?;

        cargo::build_dependencies(&project)?;

        Ok(project)
    }

    fn make_manifest(
        &self,
        crate_name: String,
        project: &Project,
        tests: &[ExpandedTest],
    ) -> Result<Manifest> {
        let source_manifest = dependencies::get_manifest(&project.source_dir);
        let workspace_manifest = dependencies::get_workspace_manifest(&project.workspace);

        let features = source_manifest
            .features
            .keys()
            .map(|feature| {
                let enable = format!("{}/{}", crate_name, feature);
                (feature.clone(), vec![enable])
            })
            .collect();

        let mut manifest = Manifest {
            package: Package {
                name: project.name.clone(),
                version: "0.0.0".to_owned(),
                edition: source_manifest.package.edition,
                publish: false,
            },
            features,
            dependencies: Map::new(),
            bins: Vec::new(),
            workspace: Some(Workspace {}),
            // Within a workspace, only the [patch] and [replace] sections in
            // the workspace root's Cargo.toml are applied by Cargo.
            patch: workspace_manifest.patch,
            replace: workspace_manifest.replace,
        };

        manifest.dependencies.extend(source_manifest.dependencies);
        manifest
            .dependencies
            .extend(source_manifest.dev_dependencies);
        manifest.dependencies.insert(
            crate_name,
            Dependency {
                version: None,
                path: Some(project.source_dir.clone()),
                default_features: false,
                features: Vec::new(),
                rest: Map::new(),
            },
        );

        manifest.bins.push(Bin {
            name: Name(project.name.to_owned()),
            path: Path::new("main.rs").to_owned(),
        });

        for expanded in tests {
            if expanded.error.is_none() {
                manifest.bins.push(Bin {
                    name: expanded.name.clone(),
                    path: project.source_dir.join(&expanded.test.path),
                });
            }
        }

        Ok(manifest)
    }

    fn make_config(&self) -> Config {
        Config {
            build: Build {
                rustflags: rustflags::make_vec(),
            },
        }
    }
}

impl Test {
    fn run(&self, project: &mut Project, name: &Name) -> Result<()> {
        let show_expected = project.has_run_at_least && project.has_compile_fail;
        let mut runner = TestRunner::default();

        let max_surge = project.surges[project.test_idx];

        if max_surge != !0 {
            project.test_idx += 1;

            let res = runner.run(&(0..max_surge), |v| {
                let duration = Duration::from_millis(v.try_into().unwrap());
                let now = Instant::now();

                message::begin_test(self, show_expected);
                check_exists(&self.path).unwrap();

                let output = cargo::build_test(project, name).unwrap();
                let success = output.status.success();
                let stdout = output.stdout;
                let stderr = normalize::diagnostics(
                    output.stderr,
                    Context {
                        krate: &name.0,
                        source_dir: &project.source_dir,
                        workspace: &project.workspace,
                    },
                );

                let check = match self.expected {
                    Expected::Available => Test::check_available,
                    // TODO: separate cases
                    Expected::Chaotic => Test::check_available,
                };

                let res = check(self, project, name, success, stdout, stderr);
                let elapsed = now.elapsed();
                if elapsed < duration {
                    Err(TestCaseError::Fail(
                        format!(
                            "chaos test failed: availability is low. Expected at least: {}, Found: {}",
                            format_duration(duration).to_string(),
                            format_duration(elapsed).to_string()
                        ).into()
                    ))
                } else {
                    res.map_err(|e| TestCaseError::Fail(format!("{}", e).into()))
                }
            })?;

            Ok(res)
        } else {
            let duration = project.durations[project.test_idx].unwrap();
            let now = Instant::now();

            message::begin_test(self, show_expected);
            check_exists(&self.path).unwrap();

            let output = cargo::build_test(project, name).unwrap();
            let success = output.status.success();
            let stdout = output.stdout;
            let stderr = normalize::diagnostics(
                output.stderr,
                Context {
                    krate: &name.0,
                    source_dir: &project.source_dir,
                    workspace: &project.workspace,
                },
            );

            let check = match self.expected {
                Expected::Available => Test::check_available,
                // TODO: separate cases
                Expected::Chaotic => Test::check_available,
            };

            let res = check(self, project, name, success, stdout, stderr);
            let elapsed = now.elapsed();
            if elapsed < duration {
                Err(Error::ChaosTestFailed(format!(
                    "availability is low. Expected at least: {}, Found: {}",
                    format_duration(duration).to_string(),
                    format_duration(elapsed).to_string()
                )))
            } else {
                res
            }
        }
    }

    fn check_available(
        &self,
        project: &Project,
        name: &Name,
        success: bool,
        build_stdout: Vec<u8>,
        variations: Variations,
    ) -> Result<()> {
        let preferred = variations.preferred();
        if !success {
            message::failed_to_build(preferred);
            return Err(Error::CargoFail);
        }

        let mut output = cargo::run_test(project, name)?;
        output.stdout.splice(..0, build_stdout);
        message::output(preferred, &output);
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::RunFailed)
        }
    }
}

fn check_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    match File::open(path) {
        Ok(_) => Ok(()),
        Err(err) => Err(Error::Open(path.to_owned(), err)),
    }
}

#[derive(Debug)]
struct ExpandedTest {
    name: Name,
    test: Test,
    error: Option<Error>,
}

fn expand_globs(tests: &[Test]) -> Vec<ExpandedTest> {
    fn glob(pattern: &str) -> Result<Vec<PathBuf>> {
        let mut paths = glob::glob(pattern)?
            .map(|entry| entry.map_err(Error::from))
            .collect::<Result<Vec<PathBuf>>>()?;
        paths.sort();
        Ok(paths)
    }

    fn bin_name(i: usize) -> Name {
        Name(format!("kaos{:03}", i))
    }

    let mut vec = Vec::new();

    for test in tests {
        let mut expanded = ExpandedTest {
            name: bin_name(vec.len()),
            test: test.clone(),
            error: None,
        };
        if let Some(utf8) = test.path.to_str() {
            if utf8.contains('*') {
                match glob(utf8) {
                    Ok(paths) => {
                        for path in paths {
                            vec.push(ExpandedTest {
                                name: bin_name(vec.len()),
                                test: Test {
                                    path,
                                    duration: expanded.test.duration,
                                    max_surge: expanded.test.max_surge,
                                    expected: expanded.test.expected,
                                },
                                error: None,
                            });
                        }
                        continue;
                    }
                    Err(error) => expanded.error = Some(error),
                }
            }
        }
        vec.push(expanded);
    }

    vec
}

impl ExpandedTest {
    fn run(self, project: &mut Project) -> Result<()> {
        match self.error {
            None => self.test.run(project, &self.name),
            Some(error) => {
                let show_expected = false;
                message::begin_test(&self.test, show_expected);
                Err(error)
            }
        }
    }
}

// Filter which test cases are run by kaos.
//
//     $ cargo test -- ui kaos=tuple_structs.rs
//
// The first argument after `--` must be the kaos test name i.e. the name of
// the function that has the #[test] attribute and calls kaos. That's to get
// Cargo to run the test at all. The next argument starting with `kaos=`
// provides a filename filter. Only test cases whose filename contains the
// filter string will be run.
fn filter(tests: &mut Vec<ExpandedTest>) {
    let filters = env::args_os()
        .flat_map(OsString::into_string)
        .filter_map(|mut arg| {
            const PREFIX: &str = "kaos=";
            if arg.starts_with(PREFIX) && arg != PREFIX {
                Some(arg.split_off(PREFIX.len()))
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    if filters.is_empty() {
        return;
    }

    tests.retain(|t| {
        filters
            .iter()
            .any(|f| t.test.path.to_string_lossy().contains(f))
    });
}
