use std::{fmt, time::Instant};

use termion::color;

pub struct Emitter {
    results: Vec<TestResult>,
    log_all: bool,
    started_at: Instant,
}

impl Emitter {
    pub fn new(log_all: bool) -> Self {
        Self {
            results: Vec::new(),
            log_all,
            started_at: Instant::now(),
        }
    }

    pub fn emit(&mut self, result: TestResult) {
        print!("{:.<75}", result.name);
        match result.outcome {
            TestOutcome::Pass => {
                println!("{}ok{}", color::Fg(color::Green), color::Fg(color::Reset));
                if self.log_all {
                    self.results.push(result)
                }
            }
            TestOutcome::Fail { .. } => {
                println!("{}FAIL{}", color::Fg(color::Red), color::Fg(color::Reset));
                self.results.push(result);
            }
            TestOutcome::Ignore => {
                println!(
                    "{}ignored{}",
                    color::Fg(color::Yellow),
                    color::Fg(color::Reset)
                );
                self.results.push(result);
            }
        }
    }
}

impl Drop for Emitter {
    fn drop(&mut self) {
        let mut passed = 0;
        let mut failed = 0;
        let mut ignored = 0;
        for result in &self.results {
            match result.outcome {
                TestOutcome::Pass => {
                    passed += 1;
                    println!("=== Test ok: {} ===", result.name);
                }
                TestOutcome::Fail { ref output } => {
                    failed += 1;
                    println!("=== Test failure: {} ===", result.name);
                    println!("{output}");
                }
                TestOutcome::Ignore => {
                    ignored += 1;
                }
            }
            if let Some(logs) = &result.logs {
                println!("Logs:");
                for entry in logs {
                    println!("{entry}");
                }
            }

            println!(
                "test result: {}. {} passed; {} ignored; {} failed; finished in {:.3?}",
                if failed == 0 {
                    format!("{}ok{}", color::Fg(color::Green), color::Fg(color::Reset))
                } else {
                    format!(
                        "{}failure{}",
                        color::Fg(color::Red),
                        color::Fg(color::Reset)
                    )
                },
                passed,
                ignored,
                failed,
                self.started_at.elapsed()
            );
        }
    }
}

pub struct TestResult {
    name: String,
    outcome: TestOutcome,
    logs: Option<Vec<LogLine>>,
}

pub struct LogLine {
    pub name: String,
    pub data: String,
}

impl LogLine {
    /// picks unique color for this line name
    fn name_color(&self) -> color::Rgb {
        // CRC hash
        let h = self.name.chars().fold(0u32, |mut h, c| {
            let highorder = h & 0xf8000000;
            h = h << 5;
            h = h ^ (highorder >> 27);
            h = h ^ c as u32;
            h
        });
        let bytes = h.to_be_bytes();
        color::Rgb(bytes[0], bytes[1], bytes[2])
    }
}

impl fmt::Display for LogLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in self.data.lines() {
            write!(
                f,
                "{}{:<10}|{} {line}",
                color::Fg(self.name_color()),
                self.name,
                color::Fg(color::Reset)
            )?;
        }

        Ok(())
    }
}

impl TestResult {
    pub fn pass(name: &str, logs: Option<Vec<LogLine>>) -> Self {
        Self {
            name: name.to_string(),
            outcome: TestOutcome::Pass,
            logs,
        }
    }

    pub fn fail(name: &str, e: String, logs: Option<Vec<LogLine>>) -> Self {
        Self {
            name: name.to_string(),
            outcome: TestOutcome::Fail { output: e },
            logs,
        }
    }

    pub fn ignore(name: &str) -> Self {
        Self {
            name: name.to_string(),
            outcome: TestOutcome::Ignore,
            logs: None,
        }
    }
}

enum TestOutcome {
    Pass,
    Fail { output: String },
    Ignore,
}
