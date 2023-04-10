use mc2::config::{Cas, Config, Remove, Update};
use mc2::runner::Runner;

fn main() {
    let mut runner = Runner::new();

    runner.configs(&[
        Config::new().update(Update::GetBeforePut),
        Config::new().remove(Remove::UnlinkParallel),
        Config::new().skip_links(true),
        Config::new().store(Cas::Lax),
        Config::new().store(Cas::NoRev),
        Config::new().store(Cas::MatchRev),
        Config::new().store(Cas::Strict),
    ]);

    runner.add(
        "update/update conflict",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").update("/path/x", |_| Some(('x', 2)));
            planner.client("B").update("/path/x", |_| Some(('x', 3)));
        },
    );

    runner.add(
        "update/update conflict (missing)",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").update("/path/y", |_| Some(('y', 2)));
            planner.client("B").update("/path/y", |_| Some(('y', 3)));
        },
    );

    runner.add(
        "update/delete conflict",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").update("/path/x", |_| Some(('x', 2)));
            planner.client("B").remove("/path/x");
        },
    );

    runner.add(
        "update/delete conflict (missing)",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").update("/path/y", |_| Some(('y', 2)));
            planner.client("B").remove("/path/y");
        },
    );

    runner.add(
        "delete, create sibling",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/x");
            planner.client("B").update("/path/y", |_| Some(('y', 2)));
        },
    );

    runner.add(
        "delete, create in parent",
        |mut db| {
            db.update("/path/to/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/to/x");
            planner.client("B").update("/path/y", |_| Some(('y', 2)));
        },
    );

    runner.add(
        "delete, create in grandparent",
        |mut db| {
            db.update("/path/to/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/to/x");
            planner.client("B").update("/y", |_| Some(('y', 2)));
        },
    );

    runner.add(
        "delete, create in child",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/x");
            planner.client("B").update("/path/to/y", |_| Some(('y', 2)));
        },
    );

    runner.add(
        "delete, create in grandchild",
        |mut db| {
            db.update("/x", |_| Some(('x', 1)));
        },
        |planner| {
            planner.client("A").remove("/x");
            planner.client("B").update("/path/to/y", |_| Some(('y', 2)));
        },
    );

    runner.add(
        "delete, update sibling",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
            db.update("/path/y", |_| Some(('y', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/x");
            planner
                .client("B")
                .update("/path/y", |doc| doc.map(|(k, n)| (k, n + 1)));
        },
    );

    runner.add(
        "delete, update in parent",
        |mut db| {
            db.update("/path/to/x", |_| Some(('x', 1)));
            db.update("/path/y", |_| Some(('y', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/to/x");
            planner
                .client("B")
                .update("/path/y", |doc| doc.map(|(k, n)| (k, n + 1)));
        },
    );

    runner.add(
        "delete, update in grandparent",
        |mut db| {
            db.update("/path/to/x", |_| Some(('x', 1)));
            db.update("/y", |_| Some(('y', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/to/x");
            planner
                .client("B")
                .update("/y", |doc| doc.map(|(k, n)| (k, n + 1)));
        },
    );

    runner.add(
        "delete, update in child",
        |mut db| {
            db.update("/path/x", |_| Some(('x', 1)));
            db.update("/path/to/y", |_| Some(('y', 1)));
        },
        |planner| {
            planner.client("A").remove("/path/x");
            planner
                .client("B")
                .update("/path/to/y", |doc| doc.map(|(k, n)| (k, n + 1)));
        },
    );

    runner.add(
        "delete, update in grandchild",
        |mut db| {
            db.update("/x", |_| Some(('x', 1)));
            db.update("/path/to/y", |_| Some(('y', 1)));
        },
        |planner| {
            planner.client("A").remove("/x");
            planner
                .client("B")
                .update("/path/to/y", |doc| doc.map(|(k, n)| (k, n + 1)));
        },
    );

    runner.run();
}
