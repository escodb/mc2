mod actor;
mod db;
mod graph;
mod path;
mod planner;
mod store;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use actor::Actor;
use db::check_consistency;
use planner::{Act, Op, Planner};
use store::Store;

fn main() {
    let mut planner = Planner::new();

    planner
        .client("A")
        .update("/path/to/x", |_| Some(vec!['a', 'b']));

    planner.client("B").update("/path/to/x", |doc| {
        let mut doc: Vec<_> = doc?.iter().cloned().rev().collect();
        doc.push('z');
        Some(doc)
    });

    let store = Store::new();
    let mut count = 0;

    for acts in planner.orderings() {
        let st = RefCell::new(store.clone());

        let mut actors: HashMap<_, _> = planner
            .clients()
            .map(|id| (id.to_string(), Actor::new(&st)))
            .collect();

        for act in acts {
            dispatch(&mut actors, act);

            if let Err(errors) = check_consistency(&st.borrow()) {
                println!("failure: {:?}", errors);
                print_store(&st.borrow());
            }
        }

        count += 1;

        if count == 1 {
            print_store(&st.borrow());
        }
    }
    println!("orderings checked: {}", count);
}

fn dispatch<T>(actors: &mut HashMap<String, Actor<T>>, act: &Act<T>)
where
    T: Clone,
{
    let actor = actors.get_mut(&act.client_id).unwrap();

    match &act.op {
        Op::Get => {
            actor.get(&act.path);
        }
        Op::Put(update) => {
            actor.put(&act.path, update);
        }
        Op::Rm => {
            actor.rm(&act.path);
        }
        Op::List => {
            actor.list(&act.path);
        }
        Op::Link(name) => {
            actor.link(&act.path, name);
        }
        Op::Unlink(name) => {
            actor.unlink(&act.path, name);
        }
    }
}

fn print_store<T>(store: &Store<T>)
where
    T: Clone + Debug,
{
    for key in store.keys() {
        if let Some((rev, value)) = store.read(key) {
            println!("    '{}' => rev: {:?}, value: {:?}", key, rev, value);
        }
    }
}
