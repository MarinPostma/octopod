use std::fmt;

#[derive(Default)]
pub struct Emitter {
    results: Vec<TestResult>,
}

impl Emitter {
    pub fn emit(&mut self, result: TestResult) {
        print!("{}\t", result.name);
        match result.outcome {
            TestOutcome::Pass => println!("PASS"),
            TestOutcome::Fail { .. } => {
                println!("FAIL");
                self.results.push(result);
            }
        }
    }
}

impl Drop for Emitter {
    fn drop(&mut self) {
        for result in &self.results {
            if let TestOutcome::Fail { ref output } = result.outcome {
                println!("=== Test failure: {} ===", result.name);
                println!("{output}");
                if let Some(logs) = &result.logs {
                    println!("Logs:");
                    for entry in logs {
                        println!("{entry}");
                    }
                }
            }
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

impl fmt::Display for LogLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in self.data.lines() {
            write!(f, "{:<10}| {line}", self.name)?;
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
}

enum TestOutcome {
    Pass,
    Fail { output: String },
}
