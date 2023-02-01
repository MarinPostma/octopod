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
                println!("{output}")
            }
        }
    }
}

pub struct TestResult {
    name: String,
    outcome: TestOutcome,
}

impl TestResult {
    pub fn pass(name: &str) -> Self {
        Self {
            name: name.to_string(),
            outcome: TestOutcome::Pass,
        }
    }

    pub fn fail(name: &str, e: &dyn std::error::Error) -> Self {
        Self {
            name: name.to_string(),
            outcome: TestOutcome::Fail {
                output: e.to_string(),
            },
        }
    }
}

enum TestOutcome {
    Pass,
    Fail { output: String },
}
