use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{mpsc, Mutex};
use std::thread;

use crate::actor::Actor;
use crate::config::Config;
use crate::db::{Checker, Db, DbStore};
use crate::planner::{Act, Client, Planner};

const SPLIT: &str = "========================================================================";

struct Scenario<T> {
    name: String,
    init: Box<dyn Fn(Client<T>)>,
    plan: Box<dyn Fn(&mut Planner<T>)>,
}

pub struct Runner<T> {
    configs: Vec<Config>,
    scenarios: Vec<Scenario<T>>,
    results: Vec<(Config, Vec<(String, bool, usize)>)>,
}

impl<T> Runner<T>
where
    T: Clone + Debug + Send,
{
    pub fn new() -> Runner<T> {
        Runner {
            configs: Vec::new(),
            scenarios: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn configs(&mut self, configs: &[Config]) {
        self.configs.extend(configs.iter().cloned());
    }

    pub fn add<S, R>(&mut self, name: &str, setup: S, run: R)
    where
        S: Fn(Client<T>) + 'static,
        R: Fn(&mut Planner<T>) + 'static,
    {
        self.scenarios.push(Scenario {
            name: name.to_string(),
            init: Box::new(setup),
            plan: Box::new(run),
        });
    }

    pub fn run(&mut self) {
        for config in &self.configs {
            println!("{}\n\n{:?}\n", SPLIT, config);
            let mut results = Vec::new();

            for scenario in &self.scenarios {
                let runner = RunnerScenario::new(config.clone(), scenario);
                let result = runner.run();
                results.push((scenario.name.clone(), result.is_pass(), result.count()));
            }
            self.results.push((config.clone(), results));
        }
        self.print_summary();
    }

    fn print_summary(&self) {
        println!("{}", SPLIT);
        println!("SUMMARY");
        println!("{}", SPLIT);
        println!("");

        let mut total = 0;

        for (config, results) in &self.results {
            println!("{:?}", config);
            for (name, passed, count) in results {
                let status = if *passed { "PASS" } else { "FAIL" };
                total += count;
                println!("    - {} ({}): {}", status, format_number(*count), name);
            }
            println!("");
        }
        println!("Total executions checked = {}", format_number(total));
        println!("");
    }
}

struct RunnerScenario<'s, T> {
    config: Config,
    scenario: &'s Scenario<T>,
    planner: Planner<T>,
}

impl<T> RunnerScenario<'_, T>
where
    T: Clone + Send,
{
    fn new(config: Config, scenario: &Scenario<T>) -> RunnerScenario<T> {
        let mut planner = Planner::new(config.clone());
        (scenario.plan)(&mut planner);

        RunnerScenario {
            config,
            scenario,
            planner,
        }
    }

    fn run(&self) -> TestResult<T>
    where
        T: Debug,
    {
        println!("Scenario: {}", self.scenario.name);

        let result = self.check_execution();
        result.print();

        println!("");

        result
    }

    fn create_store(&self) -> DbStore<T> {
        let mut planner = Planner::new(self.config.clone());
        (self.scenario.init)(planner.client("tmp"));

        let store = RefCell::new(DbStore::new(self.config.clone()));
        let mut actor = Actor::new(&store, self.config.clone());

        for act in planner.orderings().next().unwrap() {
            actor.dispatch(act);
        }

        store.into_inner()
    }

    fn check_execution(&self) -> TestResult<T> {
        let plans = Mutex::new(Box::new(self.planner.orderings().enumerate()) as PlanQueue<T>);
        let client_ids: Vec<_> = self.planner.clients().collect();
        let store = self.create_store();

        let mut supervisor = Supervisor {
            result: mpsc::channel(),
            aborts: Vec::new(),
        };

        let mut workers = Vec::new();

        for _ in 0..WORKER_COUNT {
            let (abort_send, abort_recv) = mpsc::channel();

            let worker = Worker {
                config: self.config.clone(),
                plans: &plans,
                client_ids: &client_ids,
                store: store.clone(),
                result_ch: supervisor.result.0.clone(),
                abort_ch: abort_recv,
            };

            workers.push(worker);
            supervisor.aborts.push(abort_send);
        }

        thread::scope(|scope| {
            for worker in &mut workers {
                scope.spawn(|| worker.run());
            }

            supervisor.collect_result()
        })
    }
}

const WORKER_COUNT: usize = 4;

type PlanQueue<'a, T> = Box<dyn Iterator<Item = (usize, Vec<&'a Act<T>>)> + Send + 'a>;

struct Worker<'a, 'e, T> {
    config: Config,
    plans: &'e Mutex<PlanQueue<'a, T>>,
    client_ids: &'e [&'a str],
    store: DbStore<T>,
    result_ch: mpsc::Sender<TestResult<'a, T>>,
    abort_ch: mpsc::Receiver<()>,
}

impl<'a, 'e, T> Worker<'a, 'e, T>
where
    T: Clone,
{
    fn run(&mut self) {
        let mut result = TestResult::Pass { count: 0 };

        while let Some((n, plan)) = self.next_plan() {
            if self.abort_ch.try_recv().is_ok() {
                return;
            }

            let state = RefCell::new(self.store.clone());
            let mut actors = self.create_actors(&state);
            let mut checker = Checker::new(&state);

            for (i, act) in plan.iter().enumerate() {
                actors.get_mut(&act.client_id).unwrap().dispatch(act);

                if let Err(errors) = checker.check() {
                    self.send_result(TestResult::Fail {
                        count: n + 1,
                        errors,
                        plan,
                        state: state.borrow().clone(),
                        step: i,
                    });
                    return;
                }
            }
            result = TestResult::Pass { count: n + 1 };
        }
        self.send_result(result);
    }

    fn next_plan(&self) -> Option<(usize, Vec<&'a Act<T>>)> {
        self.plans.lock().unwrap().next()
    }

    fn create_actors<'r>(&self, store: &'r RefCell<DbStore<T>>) -> HashMap<String, Actor<'r, T>> {
        self.client_ids
            .iter()
            .map(|name| (name.to_string(), Actor::new(store, self.config.clone())))
            .collect()
    }

    fn send_result(&self, result: TestResult<'a, T>) {
        self.result_ch.send(result).unwrap();
    }
}

type Channel<T> = (mpsc::Sender<T>, mpsc::Receiver<T>);

struct Supervisor<'a, T> {
    result: Channel<TestResult<'a, T>>,
    aborts: Vec<mpsc::Sender<()>>,
}

impl<'a, T> Supervisor<'a, T> {
    fn collect_result(&self) -> TestResult<'a, T> {
        let mut result = TestResult::Pass { count: 0 };
        let mut finished = 0;

        for worker_result in &self.result.1 {
            if worker_result.is_pass() {
                if worker_result.count() > result.count() {
                    result = worker_result
                }
                finished += 1;
                if finished == self.aborts.len() {
                    break;
                }
            } else {
                result = worker_result;
                self.abort();
                break;
            }
        }
        result
    }

    fn abort(&self) {
        for abort in &self.aborts {
            abort.send(()).unwrap();
        }
    }
}

enum TestResult<'a, T> {
    Pass {
        count: usize,
    },
    Fail {
        count: usize,
        errors: Vec<String>,
        state: DbStore<T>,
        plan: Vec<&'a Act<T>>,
        step: usize,
    },
}

impl<T> TestResult<'_, T> {
    fn is_pass(&self) -> bool {
        match self {
            TestResult::Pass { .. } => true,
            TestResult::Fail { .. } => false,
        }
    }

    fn count(&self) -> usize {
        match self {
            TestResult::Pass { count } => *count,
            TestResult::Fail { count, .. } => *count,
        }
    }

    fn print(&self)
    where
        T: Clone + Debug,
    {
        let status = if self.is_pass() { "PASS" } else { "FAIL" };
        println!("    result: {}", status);
        println!("    checked executions: {}", format_number(self.count()));

        if let TestResult::Fail {
            errors,
            state,
            plan,
            step,
            ..
        } = self
        {
            println!("    errors:");
            for error in errors {
                println!("        - {}", error);
            }
            println!("    state:");
            for key in state.keys() {
                let value = format_value(state.read(key));
                println!("        '{}' => {}", key, value);
            }
            println!("    execution:");
            for (i, act) in plan.iter().enumerate() {
                if i == *step {
                    println!("    ==> {:?}", act);
                } else {
                    println!("        {:?}", act);
                }
            }
        }
    }
}

fn format_number(n: usize) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|byte| std::str::from_utf8(byte))
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(",")
}

fn format_value<T>(value: Option<(usize, Option<Db<T>>)>) -> String
where
    T: Debug,
{
    if let Some((rev, value)) = value {
        if let Some(value) = value {
            format!("{{ rev: {}, value: {:?} }}", rev, value)
        } else {
            format!("{{ rev: {}, value: <null> }}", rev)
        }
    } else {
        String::from("<null>")
    }
}
