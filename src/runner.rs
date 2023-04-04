use std::cell::RefCell;
use std::collections::HashMap;

use crate::actor::Actor;
use crate::config::Config;
use crate::db::{check_consistency, DbStore};
use crate::planner::{Client, Planner};

struct Scenario<T> {
    name: String,
    init: Box<dyn Fn(Client<T>)>,
    plan: Box<dyn Fn(&mut Planner<T>)>,
}

impl<T> Scenario<T>
where
    T: Clone + std::fmt::Debug,
{
    fn run(&self) {
        let mut planner = Planner::new();
        (self.plan)(&mut planner);

        let store = self.create_store();
        let mut count: usize = 0;

        for plan in planner.orderings() {
            count += 1;
            let state = RefCell::new(store.clone());

            let mut actors: HashMap<_, _> = planner
                .clients()
                .map(|name| (name.to_string(), Actor::new(&state, Config::new())))
                .collect();

            for act in plan {
                actors.get_mut(&act.client_id).unwrap().dispatch(act);
                check_consistency(&state.borrow()).unwrap();
            }
        }
        println!("    checked executions: {}", count);
        println!("");
    }

    fn create_store(&self) -> DbStore<T> {
        let mut planner = Planner::new();
        (self.init)(planner.client("tmp"));

        let store = RefCell::new(DbStore::new());
        let mut actor = Actor::new(&store, Config::new());

        for act in planner.orderings().next().unwrap() {
            actor.dispatch(act);
        }

        store.into_inner()
    }
}

pub struct Runner<T> {
    scenarios: Vec<Scenario<T>>,
}

impl<T> Runner<T>
where
    T: Clone + std::fmt::Debug,
{
    pub fn new() -> Runner<T> {
        Runner {
            scenarios: Vec::new(),
        }
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
        for scenario in &self.scenarios {
            println!("Scenario: {}", scenario.name);
            scenario.run();
        }
    }
}
