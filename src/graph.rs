use std::iter;

pub type Id = usize;

#[derive(Debug)]
pub struct Graph<T> {
    nodes: Vec<Node<T>>,
}

#[derive(Debug)]
struct Node<T> {
    id: Id,
    deps: Vec<Id>,
    value: T,
}

type IdIter = Box<dyn Iterator<Item = Id>>;

impl<T> Graph<T> {
    pub fn new() -> Graph<T> {
        Graph { nodes: Vec::new() }
    }

    pub fn add(&mut self, deps: &[Id], value: T) -> Id {
        let node_id = self.nodes.len() + 1;

        self.nodes.push(Node {
            id: node_id,
            deps: deps.into(),
            value,
        });

        node_id
    }

    pub fn orderings(&self) -> impl Iterator<Item = impl Iterator<Item = &T>> {
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| (node.id, node.deps.clone()))
            .collect();

        permute(nodes).map(|order| order.map(|id| &self.nodes[id - 1].value))
    }
}

fn permute(nodes: Vec<(Id, Vec<Id>)>) -> Box<dyn Iterator<Item = IdIter>> {
    if nodes.is_empty() {
        let inner = Box::new(iter::empty()) as IdIter;
        return Box::new(iter::once(inner));
    }

    let available: Vec<_> = nodes
        .iter()
        .filter(|(_, deps)| deps.is_empty())
        .map(|(node_id, _)| *node_id)
        .collect();

    let states = available.into_iter().flat_map(move |action| {
        let remaining: Vec<_> = nodes
            .iter()
            .filter(|(node_id, _)| *node_id != action)
            .map(|(node_id, deps)| {
                let filtered = deps.iter().cloned().filter(|dep| *dep != action).collect();
                (*node_id, filtered)
            })
            .collect();

        permute(remaining).map(move |others| {
            let chain = iter::once(action).chain(others);
            Box::new(chain) as IdIter
        })
    });

    Box::new(states)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::fmt::Debug;

    type NodeSpec<'a, T> = (&'a str, T, &'a [&'a str]);

    pub fn check_graph<T>(graph: &Graph<T>, nodes: &[NodeSpec<T>])
    where
        T: Debug + PartialEq,
    {
        let mut mapping: HashMap<String, Id> = HashMap::new();

        assert_eq!(
            graph.nodes.len(),
            nodes.len(),
            "graph expected to have {} nodes but has {}",
            nodes.len(),
            graph.nodes.len()
        );

        for (key, value, deps) in nodes {
            let opt_node = graph.nodes.iter().find(|node| &node.value == value);
            assert!(opt_node.is_some(), "no node matching {:?}", value);

            let node = opt_node.unwrap();
            mapping.insert(key.to_string(), node.id);

            let dep_ids: HashSet<_> = deps
                .iter()
                .map(|name| mapping.get(*name).unwrap())
                .collect();

            let actual_ids: HashSet<_> = node.deps.iter().collect();

            assert_eq!(actual_ids.len(), node.deps.len());

            assert_eq!(
                actual_ids, dep_ids,
                "dependencies for {:?} do not match node {:?}",
                value, node
            );
        }

        assert_eq!(mapping.len(), nodes.len());
    }

    fn collect_orderings<T>(graph: &Graph<T>) -> Vec<Vec<&T>> {
        graph.orderings().map(|o| o.collect()).collect()
    }

    #[test]
    fn orders_a_single_action() {
        let mut graph = Graph::new();
        graph.add(&[], 'a');
        let orderings = collect_orderings(&graph);

        assert_eq!(orderings, [vec![&'a']]);
    }

    #[test]
    fn orders_two_concurrent_events() {
        let mut graph = Graph::new();
        graph.add(&[], 'a');
        graph.add(&[], 'b');
        let orderings = collect_orderings(&graph);

        assert_eq!(orderings, [vec![&'a', &'b'], vec![&'b', &'a']]);
    }

    #[test]
    fn orders_two_sequential_events() {
        let mut graph = Graph::new();
        let a = graph.add(&[], 'a');
        graph.add(&[a], 'b');
        let orderings = collect_orderings(&graph);

        assert_eq!(orderings, [vec![&'a', &'b']]);
    }

    #[test]
    fn orders_a_diamond_shaped_graph() {
        let mut graph = Graph::new();

        let a = graph.add(&[], 'a');
        let b = graph.add(&[a], 'b');
        let c = graph.add(&[a], 'c');
        graph.add(&[b, c], 'd');

        let orderings = collect_orderings(&graph);

        assert_eq!(
            orderings,
            [vec![&'a', &'b', &'c', &'d'], vec![&'a', &'c', &'b', &'d']]
        );
    }

    #[test]
    fn orders_two_sets_of_unconnected_sequences() {
        let mut graph = Graph::new();

        for chain in vec![vec!['a', 'b'], vec!['c', 'd', 'e']] {
            let mut deps = vec![];
            for act in chain {
                deps = vec![graph.add(&deps, act)];
            }
        }
        let orderings = collect_orderings(&graph);

        assert_eq!(
            orderings,
            [
                vec![&'a', &'b', &'c', &'d', &'e'],
                vec![&'a', &'c', &'b', &'d', &'e'],
                vec![&'a', &'c', &'d', &'b', &'e'],
                vec![&'a', &'c', &'d', &'e', &'b'],
                vec![&'c', &'a', &'b', &'d', &'e'],
                vec![&'c', &'a', &'d', &'b', &'e'],
                vec![&'c', &'a', &'d', &'e', &'b'],
                vec![&'c', &'d', &'a', &'b', &'e'],
                vec![&'c', &'d', &'a', &'e', &'b'],
                vec![&'c', &'d', &'e', &'a', &'b']
            ]
        );
    }

    #[test]
    fn orders_a_top_level_update_operation() {
        let mut graph = Graph::new();

        let reads = [graph.add(&[], "LIST /"), graph.add(&[], "GET /x")];
        let link = graph.add(&reads, "LINK / x");
        graph.add(&[link], "PUT /x {}");

        let orderings = collect_orderings(&graph);

        assert_eq!(
            orderings,
            [
                vec![&"LIST /", &"GET /x", &"LINK / x", &"PUT /x {}"],
                vec![&"GET /x", &"LIST /", &"LINK / x", &"PUT /x {}"],
            ]
        );
    }

    #[test]
    fn orders_a_top_level_update_operation_with_deferred_get() {
        let mut graph = Graph::new();

        let list = graph.add(&[], "LIST /");
        let link = graph.add(&[list], "LINK / x");
        let get = graph.add(&[], "GET /x");
        graph.add(&[get, link], "PUT /x {}");

        let orderings = collect_orderings(&graph);

        assert_eq!(
            orderings,
            [
                vec![&"LIST /", &"LINK / x", &"GET /x", &"PUT /x {}"],
                vec![&"LIST /", &"GET /x", &"LINK / x", &"PUT /x {}"],
                vec![&"GET /x", &"LIST /", &"LINK / x", &"PUT /x {}"],
            ]
        );
    }

    #[test]
    fn orders_a_nested_update_operation() {
        let mut graph = Graph::new();

        let reads: Vec<_> = ["GET /path/x", "LIST /path/", "LIST /"]
            .into_iter()
            .map(|action| graph.add(&[], action))
            .collect();

        let links: Vec<_> = ["LINK /path/ x", "LINK / path/"]
            .into_iter()
            .map(|action| graph.add(&reads, action))
            .collect();

        graph.add(&links, "PUT /path/x {}");

        let orderings = collect_orderings(&graph);

        assert_eq!(
            orderings,
            [
                vec![
                    &"GET /path/x",
                    &"LIST /path/",
                    &"LIST /",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"GET /path/x",
                    &"LIST /path/",
                    &"LIST /",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"GET /path/x",
                    &"LIST /",
                    &"LIST /path/",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"GET /path/x",
                    &"LIST /",
                    &"LIST /path/",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /path/",
                    &"GET /path/x",
                    &"LIST /",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /path/",
                    &"GET /path/x",
                    &"LIST /",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /path/",
                    &"LIST /",
                    &"GET /path/x",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /path/",
                    &"LIST /",
                    &"GET /path/x",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /",
                    &"GET /path/x",
                    &"LIST /path/",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /",
                    &"GET /path/x",
                    &"LIST /path/",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /",
                    &"LIST /path/",
                    &"GET /path/x",
                    &"LINK /path/ x",
                    &"LINK / path/",
                    &"PUT /path/x {}"
                ],
                vec![
                    &"LIST /",
                    &"LIST /path/",
                    &"GET /path/x",
                    &"LINK / path/",
                    &"LINK /path/ x",
                    &"PUT /path/x {}"
                ]
            ]
        );
    }

    fn example_graph() -> Graph<usize> {
        let mut graph = Graph::new();

        let n3 = graph.add(&[], 3);
        let n5 = graph.add(&[], 5);
        let n7 = graph.add(&[], 7);
        let n0 = graph.add(&[n3, n7], 0);
        let n1 = graph.add(&[n5, n7], 1);
        let _n2 = graph.add(&[n1], 2);
        let _n4 = graph.add(&[n1, n3], 4);
        let _n6 = graph.add(&[n0, n1], 6);

        graph
    }

    #[test]
    fn returns_a_uniqe_set_of_orderings() {
        let graph = example_graph();

        let orderings = collect_orderings(&graph);
        assert_eq!(orderings.len(), 150);

        let unique: HashSet<_> = orderings.iter().collect();
        assert_eq!(unique.len(), orderings.len());
    }

    #[test]
    fn keeps_sequential_nodes_in_order() {
        let graph = example_graph();

        let pairs = [
            (0, 3),
            (0, 7),
            (1, 5),
            (1, 7),
            (2, 1),
            (2, 5),
            (2, 7),
            (4, 1),
            (4, 3),
            (4, 5),
            (4, 7),
            (6, 0),
            (6, 1),
            (6, 3),
            (6, 5),
            (6, 7),
        ];

        for order in graph.orderings() {
            let seq: Vec<_> = order.collect();
            for (a, b) in pairs {
                let pos_a = seq.iter().position(|n| **n == a);
                let pos_b = seq.iter().position(|n| **n == b);
                assert!(pos_a > pos_b, "node {} appears before node {}", a, b);
            }
        }
    }

    #[test]
    fn allows_concurrent_nodes_in_any_order() {
        let graph = example_graph();

        assert!(graph.orderings().any(|order| {
            let seq: Vec<_> = order.collect();
            let pos_4 = seq.iter().position(|n| **n == 4);
            let pos_6 = seq.iter().position(|n| **n == 6);
            pos_4 < pos_6
        }));

        assert!(graph.orderings().any(|order| {
            let seq: Vec<_> = order.collect();
            let pos_4 = seq.iter().position(|n| **n == 4);
            let pos_6 = seq.iter().position(|n| **n == 6);
            pos_4 > pos_6
        }));
    }
}
