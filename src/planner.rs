#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fmt;

use crate::graph::{Graph, Id};
use crate::path::Path;

#[derive(PartialEq)]
pub struct Act<T> {
    pub client_id: String,
    pub path: Path,
    pub op: Op<T>,
}

impl<T> Act<T> {
    fn new(client_id: &str, path: Path, op: Op<T>) -> Act<T> {
        Act {
            client_id: client_id.to_string(),
            path,
            op,
        }
    }
}

impl<T> fmt::Debug for Act<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Act[{}: ", self.client_id)?;

        match &self.op {
            Op::Get => write!(f, "get('{}')", self.path)?,
            Op::Put(_) => write!(f, "put('{}')", self.path)?,
            Op::Rm => write!(f, "rm('{}')", self.path)?,
            Op::List => write!(f, "list('{}')", self.path)?,
            Op::Link(name) => write!(f, "link('{}', '{}')", self.path, name)?,
            Op::Unlink(name) => write!(f, "unlink('{}', '{}')", self.path, name)?,
        };

        write!(f, "]")
    }
}

pub enum Op<T> {
    Get,
    Put(Box<dyn Fn(Option<T>) -> Option<T>>),
    Rm,
    List,
    Link(String),
    Unlink(String),
}

impl<T> PartialEq for Op<T> {
    fn eq(&self, other: &Op<T>) -> bool {
        match (self, other) {
            (Op::Get, Op::Get) => true,
            (Op::Put(_), Op::Put(_)) => true,
            (Op::Rm, Op::Rm) => true,
            (Op::List, Op::List) => true,
            (Op::Link(a), Op::Link(b)) if a == b => true,
            (Op::Unlink(a), Op::Unlink(b)) if a == b => true,
            _ => false,
        }
    }
}

pub struct Planner<T> {
    graph: Graph<Act<T>>,
    clients: BTreeSet<String>,
}

impl<T> Planner<T> {
    pub fn new() -> Planner<T> {
        Planner {
            graph: Graph::new(),
            clients: BTreeSet::new(),
        }
    }

    pub fn client(&mut self, id: &str) -> Client<T> {
        self.clients.insert(id.to_string());
        Client::new(&mut self.graph, id)
    }

    pub fn clients(&self) -> impl Iterator<Item = &str> {
        self.clients.iter().map(|s| s.as_ref())
    }

    pub fn orderings(&self) -> impl Iterator<Item = impl Iterator<Item = &Act<T>>> {
        self.graph.orderings()
    }
}

pub struct Client<'a, T> {
    id: String,
    graph: &'a mut Graph<Act<T>>,
}

impl<'a, T> Client<'a, T> {
    fn new(graph: &'a mut Graph<Act<T>>, id: &str) -> Client<'a, T> {
        Client {
            id: id.to_string(),
            graph,
        }
    }

    fn act<P>(&self, path: P, op: Op<T>) -> Act<T>
    where
        P: Into<Path>,
    {
        Act::new(&self.id, path.into(), op)
    }

    fn do_reads(&mut self, path: &Path) -> Vec<Id> {
        let mut reads: Vec<_> = path
            .dirs()
            .map(|dir| self.graph.add(&[], self.act(dir, Op::List)))
            .collect();

        let get = self.act(path, Op::Get);
        reads.push(self.graph.add(&[], get));

        reads
    }

    pub fn update<F>(&mut self, key: &str, update: F)
    where
        F: Fn(Option<T>) -> Option<T> + 'static,
    {
        let path = Path::from(key);
        let reads = self.do_reads(&path);

        let links: Vec<_> = path
            .links()
            .map(|(dir, name)| {
                let link = self.act(dir, Op::Link(name.to_string()));
                self.graph.add(&reads, link)
            })
            .collect();

        let put = self.act(&path, Op::Put(Box::new(update)));
        self.graph.add(&links, put);
    }

    pub fn remove(&mut self, key: &str) {
        let path = Path::from(key);
        let reads = self.do_reads(&path);

        let mut op = self.graph.add(&reads, self.act(&path, Op::Rm));

        for (dir, name) in path.links().rev() {
            let unlink = self.act(dir, Op::Unlink(name.to_string()));
            op = self.graph.add(&[op], unlink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::tests::check_graph;

    #[test]
    fn returns_the_ids_of_registered_clients() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("alice").update("/x", |_| Some(vec!['x']));
        planner.client("bob").remove("/y");

        let clients: Vec<_> = planner.clients().collect();
        assert_eq!(clients, ["alice", "bob"]);
    }

    #[test]
    fn plans_a_top_level_document_update() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("A").update("/x.json", |doc| doc);

        check_graph(
            &planner.graph,
            &[
                ("get", Act::new("A", "/x.json".into(), Op::Get), &[]),
                ("list", Act::new("A", "/".into(), Op::List), &[]),
                (
                    "link",
                    Act::new("A", "/".into(), Op::Link("x.json".into())),
                    &["get", "list"],
                ),
                (
                    "put",
                    Act::new("A", "/x.json".into(), Op::Put(Box::new(|d| d))),
                    &["link"],
                ),
            ],
        );
    }

    #[test]
    fn plans_an_update_in_a_top_level_directory() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("A").update("/path/x.json", |doc| doc);

        check_graph(
            &planner.graph,
            &[
                ("get", Act::new("A", "/path/x.json".into(), Op::Get), &[]),
                ("list1", Act::new("A", "/".into(), Op::List), &[]),
                ("list2", Act::new("A", "/path/".into(), Op::List), &[]),
                (
                    "link1",
                    Act::new("A", "/".into(), Op::Link("path/".into())),
                    &["get", "list1", "list2"],
                ),
                (
                    "link2",
                    Act::new("A", "/path/".into(), Op::Link("x.json".into())),
                    &["get", "list1", "list2"],
                ),
                (
                    "put",
                    Act::new("A", "/path/x.json".into(), Op::Put(Box::new(|d| d))),
                    &["link1", "link2"],
                ),
            ],
        );
    }

    #[test]
    fn plans_an_update_in_a_nested_directory() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("A").update("/path/to/x.json", |doc| doc);

        check_graph(
            &planner.graph,
            &[
                ("get", Act::new("A", "/path/to/x.json".into(), Op::Get), &[]),
                ("list1", Act::new("A", "/".into(), Op::List), &[]),
                ("list2", Act::new("A", "/path/".into(), Op::List), &[]),
                ("list3", Act::new("A", "/path/to/".into(), Op::List), &[]),
                (
                    "link1",
                    Act::new("A", "/".into(), Op::Link("path/".into())),
                    &["get", "list1", "list2", "list3"],
                ),
                (
                    "link2",
                    Act::new("A", "/path/".into(), Op::Link("to/".into())),
                    &["get", "list1", "list2", "list3"],
                ),
                (
                    "link3",
                    Act::new("A", "/path/to/".into(), Op::Link("x.json".into())),
                    &["get", "list1", "list2", "list3"],
                ),
                (
                    "put",
                    Act::new("A", "/path/to/x.json".into(), Op::Put(Box::new(|d| d))),
                    &["link1", "link2", "link3"],
                ),
            ],
        );
    }

    #[test]
    fn plans_a_top_level_document_deletion() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("A").remove("/y.json");

        check_graph(
            &planner.graph,
            &[
                ("get", Act::new("A", "/y.json".into(), Op::Get), &[]),
                ("list", Act::new("A", "/".into(), Op::List), &[]),
                (
                    "rm",
                    Act::new("A", "/y.json".into(), Op::Rm),
                    &["get", "list"],
                ),
                (
                    "unlink",
                    Act::new("A", "/".into(), Op::Unlink("y.json".into())),
                    &["rm"],
                ),
            ],
        );
    }

    #[test]
    fn plans_a_deletion_in_a_nested_directory() {
        let mut planner: Planner<Vec<char>> = Planner::new();

        planner.client("A").remove("/path/to/y.json");

        check_graph(
            &planner.graph,
            &[
                ("get", Act::new("A", "/path/to/y.json".into(), Op::Get), &[]),
                ("list1", Act::new("A", "/".into(), Op::List), &[]),
                ("list2", Act::new("A", "/path/".into(), Op::List), &[]),
                ("list3", Act::new("A", "/path/to/".into(), Op::List), &[]),
                (
                    "rm",
                    Act::new("A", "/path/to/y.json".into(), Op::Rm),
                    &["get", "list1", "list2", "list3"],
                ),
                (
                    "unlink1",
                    Act::new("A", "/path/to/".into(), Op::Unlink("y.json".into())),
                    &["rm"],
                ),
                (
                    "unlink2",
                    Act::new("A", "/path/".into(), Op::Unlink("to/".into())),
                    &["unlink1"],
                ),
                (
                    "unlink3",
                    Act::new("A", "/".into(), Op::Unlink("path/".into())),
                    &["unlink2"],
                ),
            ],
        );
    }
}
