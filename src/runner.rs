use std::cell::RefCell;
use std::collections::HashMap;

use crate::actor::Actor;
use crate::config::Config;
use crate::db::{Checker, DbStore};
use crate::planner::{Client, Planner};

const SPLIT: &str = "========================================================================";

struct Scenario<T> {
    name: String,
    init: Box<dyn Fn(Client<T>)>,
    plan: Box<dyn Fn(&mut Planner<T>)>,
}

pub struct Runner<T> {
    configs: Vec<Config>,
    scenarios: Vec<Scenario<T>>,
}

impl<T> Runner<T>
where
    T: Clone + std::fmt::Debug,
{
    pub fn new() -> Runner<T> {
        Runner {
            configs: Vec::new(),
            scenarios: Vec::new(),
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

    pub fn run(&self) {
        for config in &self.configs {
            println!("{}\n\n{:?}\n", SPLIT, config);

            for scenario in &self.scenarios {
                let runner = RunnerScenario::new(config.clone(), scenario);
                runner.run();
            }
        }
    }
}

struct RunnerScenario<'a, T> {
    config: Config,
    scenario: &'a Scenario<T>,
}

impl<T> RunnerScenario<'_, T>
where
    T: Clone,
{
    fn new(config: Config, scenario: &Scenario<T>) -> RunnerScenario<T> {
        RunnerScenario { config, scenario }
    }

    fn run(&self) {
        println!("Scenario: {}", self.scenario.name);

        let count = self.check_execution();

        println!("    checked executions: {}", format_number(count));
        println!("");
    }

    fn create_store(&self) -> DbStore<T> {
        let mut planner = Planner::new(self.config.clone());
        (self.scenario.init)(planner.client("tmp"));

        let store = RefCell::new(DbStore::new());
        let mut actor = Actor::new(&store, self.config.clone());

        for act in planner.orderings().next().unwrap() {
            actor.dispatch(act);
        }

        store.into_inner()
    }

    fn check_execution(&self) -> usize {
        let mut planner = Planner::new(self.config.clone());
        (self.scenario.plan)(&mut planner);

        let store = self.create_store();
        let mut count: usize = 0;

        for plan in planner.orderings() {
            count += 1;
            let state = RefCell::new(store.clone());
            let mut checker = Checker::new(&state);

            let mut actors: HashMap<_, _> = planner
                .clients()
                .map(|name| (name.to_string(), Actor::new(&state, self.config.clone())))
                .collect();

            for act in plan {
                actors.get_mut(&act.client_id).unwrap().dispatch(act);

                if let Err(_) = checker.check() {
                    println!("    result: FAIL");
                    return count;
                }
            }
        }
        println!("    result: PASS");
        count
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
