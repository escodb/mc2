# mc2

This is a special-purpose model checker that verifies the implementation
constraints discussed in [the design of VaultDB][1]. It simulates the behaviour
of sets of clients concurrently interacting with a data store, executing every
possible ordering of effects and checking whether each execution causes any
consistency violations. We use this to confirm the analysis of the design
document and to discover behavioural requirements that clients and data stores
must abide by for the design to work correctly.

[1]: https://github.com/vault-db/design


## Usage

`mc2` is written in [Rust][2] and requires the `cargo` toolchain to run. Check
that the tests pass and then run the program with these commands:

    $ cargo test
    $ cargo run --release

[2]: https://www.rust-lang.org/

Running the model checker is CPU intensive and benefits substantially from being
run in `--release` mode. It will run much slower under Rust's development/debug
settings.


## Implementation

The model checker is composed of a number of components that simulate the
behaviour of storage servers and VaultDB clients. Some of these components allow
the actions of some agent to be specified in terms of a dependency graph so that
all possible orderings of events can be generated from this graph. The
components of the system are explained below.


### Storage

VaultDB is designed to work on top of a blob store with [compare-and-swap][3]
(CAS) behaviour. In typical implementations this is accomplished by associating
a version ID with each stored item, that ID being an incrementing counter, a
content hash, or combination of the two. In our model, a counter is used. All
writes must include a version ID that matches the currently stored ID for that
item in order to be accepted.

[3]: https://en.wikipedia.org/wiki/Compare-and-swap

The storage is implemented by the type `Store<K, V>` which is initialised using
a `Config` object. (`Config` is used to control the behaviour of various system
components; the available options are described below.)


```rs
let mut store = Store::new(Config::new());
```

Reading from a key that has never been assigned a value returns `None`:

```rs
let mut x = store.read("x");
// x == None
```

Writing to a key requires an optional version ID, and the desired new value.
Here we write the value `'a'` to the key `"x"`, with `None` as the version ID as
this is the first write to this key. The `write()` call returns `Some(rev)`
where `rev` is the new version ID. After the write, reading from the key `"x"`
returns `Some((rev, value))` where `rev` is the current version ID and `value`
is an `Option` containing the key's current value.

```rs
let w1 = store.write("x", None, 'a');
// w1 == Some(1)
x = store.read("x");
// x == Some((1, Some('a')))
```

Since this type is simulating what would be a network-accessed storage service
in real life, it returns a _copy_ of the stored data, not a reference into the
data inside the `Store`. It requires the key and value types to both be `Clone`.
If a client retrieves a value and makes modifications to its copy, those changes
are not visible to other clients until the value is written back to the `Store`
in full.

Once a key exists in the store, writing to it using `None` as the version ID
will cause `write()` to return `None`, and will leave the store unmodified. If
`write()` is called using a matching version ID, the write succeeds and returns
another `Some(rev)`.

```rs
let w2 = store.write("x", None, 'b');
// w2 == None
x = store.read("x");
// x == Some((1, Some('a')))

let w3 = store.write("x", w1, 'c');
// w3 == Some(2)
x = store.read("x");
// x == Some((2, Some('c')))
```

The same restriction applies to the `remove()` method; it must be called with a
matching version ID or it will return `None` and leave the key unmodified. On a
valid `remove()` call, the key's value is removed (leaving `None` in its place)
but its version ID is maintained. Reading the key will continue to return
`Some((rev, None))`, rather than `None` as would happen if the key did not
exist.

```rs
let r1 = store.remove("x", None);
// r1 == None
x = store.read("x");
// x == Some((2, Some('c')))

let r2 = store.remove("x", w3);
// r2 == Some(3)
x = store.read("x");
// x == Some((3, None))
```

This means that once a key has its value removed, future writes to it must
continue to match the key's version ID. Passing `None` as the version ID for a
`write()` following a `remove()` will fail.

```rs
let w4 = store.write("x", None, 'd');
// w4 == None
x = store.read("x");
// x == Some((3, None))

let w5 = store.write("x", r2, 'e');
// w5 == Some(4)
x = store.read("x");
// x == Some((4, Some('e')))
```

This is not how all data stores we have in mind as implementation targets handle
writes to deleted keys and so the version matching requirements for deleted keys
is configurable. A full description of the `Config` API is given below.


### Paths and values

In VaultDB, the keys of the store are _paths_, analogous to filesystem paths,
and the values are either _documents_ (the primary content being stored by the
system) or _directories_ (lists of items that exist under a certain directory
path). A consistency requirement is that for any document that exists, its path
must be fully represented by links in its parent directories.

For example, if a document exists with path `/path/to/x`, then the directory
`/path/to/` must contain the item `x`, the directory `/path/` must contain the
item `to/`, and the directory `/` must contain the item `path/`.

The `Path` type implements logic for parsing these paths and enumerating the set
of directory-item links that must exist for them.

```rs
let path = Path::from("/path/to/x");

for (dir, name) in path.links() {
    println!("dir = {:?}, name = {:?}", dir, name);
}

// -> dir = "/", name = "path/"
//    dir = "/path/", name = "to/"
//    dir = "/path/to/", name = "x"
```

For performance reasons, these sets of links are cached inside the `Path` object
and are not recalculated for each call to `links()`.

The values stored during model checking are represented by the enum `Db<T>`
which is either `Doc(T)` or `Dir(BTreeSet<String>)`. So the full type of VaultDB
stores is `Store<Path, Db<T>>`.


### Actors

The behaviour of client processes interacting with a `Store` is modelled by
`Actor` objects. An `Actor` represents a client's in-memory state based on the
responses it's received from a `Store`, keeping track of the latest version ID
and value it saw for each key. An `Actor<T>` interacts with a `Store<Path, Db<T:
Clone>>` and has the following methods that correspond with the low-level item
operations in the VaultDB design.

- `get(path: &Path) -> Option<T>`: loads the given document and its version ID
  from the store.

- `put<F>(path: &Path, update: F) where F: Fn(Option<T>) -> Option<T>`: passes
  the document value it has in memory through the `update()` closure to produce
  a new value, and writes that value back to the store.

- `rm(path: &Path)`: removes the given document key from the store, and retains
  the resulting version ID for future writes.

- `list(path: &Path) -> Option<BTreeSet<String>>`: loads the given directory and
  its version ID into memory from the store.

- `link(path: &Path, entry: &str)`: adds the item named by `entry` to the
  directory stored at `path` and writes the directory back to the store.

- `unlink(path: &Path, entry: &str)`: removes the item named by `entry` from the
  directory stored at `path` and writes the directory back to the store.

If any of the writes attempted by an `Actor` results in a conflict due to a
version ID mismatch, the `Actor` will not perform any further actions. This
models the design intent that a client should react to a conflict by halting the
current execution and retrying whatever high-level workflow it was executing
from the beginning, rather than by retrying only the conflicting write.

To show that this will not produce consistency violations, we just need to check
all possible interleavings of `update()`/`remove()` calls from different
clients, rather than having them implement the retry logic. If each of the above
`Actor` methods leaves the store in a consistent state, then it is safe if a
client crashes or if it restarts whatever process it was running from the
beginning.

For example, the workflow for updating the document `/path/to/x` requires
creating all the necessary links, using `list()` to load a directory into memory
and `link()` to update it, and it requires loading the document into memory
using `get()` and then updating it with `put()`.

```rs
let config = Config::new();
let store = RefCell::new(Store::new(config.clone()));
let mut actor = Actor::new(&store, config.clone());

let path = Path::from("/path/to/x");

for (dir, name) in path.links() {
    let dir_path = Path::from(dir);
    actor.list(&dir_path);
    actor.link(&dir_path, name);
}

actor.get(&path);
actor.put(&path, |_| Some(('a', 1)));
```

The `put()` and `link()` calls don't need to handle version IDs; those are
tracked internally by a cache that the `Actor` holds to reflect what it knows
about the `Store` state. Here the `put()` closure ignores the document's current
state because we're creating it for the first time, but in general this closure
would transform its input in some way to produce the new value.

We can inspect the state of the `Store` after running these operations and check
it contains what we expect.

```rs
for key in store.borrow().keys() {
    let (rev, value) = store.borrow().read(key).unwrap();
    println!("key = {:?}, rev = {:?}, value = {:?}", key, rev, value);
}

// -> key = Path(/), rev = 1, value = Some(Dir({"path/"}))
//    key = Path(/path/), rev = 1, value = Some(Dir({"to/"}))
//    key = Path(/path/to/), rev = 1, value = Some(Dir({"x"}))
//    key = Path(/path/to/x), rev = 1, value = Some(Doc(('a', 1)))
```

Note also that the `get()` and `list()` methods return the values the `Actor`
currently has in its memory for documents and directories.

```rs
actor.list("/")             // -> Some({"path/"})
actor.list("/path/")        //    Some({"to/"})
actor.list("/path/to/")     //    Some({"x"})
actor.get(&path)            //    Some(('a', 1))
```


### Execution planning

Above we gave an example of performing an `update()` for a document using the
`get()`, `put()`, `list()` and `link()` operations. This implementation performs
all operations sequentially, but in VaultDB we would like to perform as many of
these operations as possible concurrently to reduce latency. We also need to
make sure that when multiple clients access the storage at the same time, that
they behave in a way that will not create consistency violations.

To perform these checks, we generate all possible orderings of `Actor`
operations invoked. The `Planner` type provides a high-level API for expressing
the operations we want clients to perform, and generates all possible ways of
executing those operations. For example, we can say we want one client named `A`
to update a document:

```rs
let config = Config::new();
let mut planner = Planner::new(config.clone());

planner.client("A").update("/path/x", |_| Some(('a', 1)));
```

And from this we can generate multiple possible sequences of `Actor` method
calls that will produce the desired outcome:

```rs
for (i, plan) in planner.orderings().enumerate() {
    println!("plan #{}:", i + 1);
    for act in plan {
        println!("    {:?}", act);
    }
}

// -> plan #1:
//        Act<A: list('/')>
//        Act<A: list('/path/')>
//        Act<A: get('/path/x')>
//        Act<A: link('/', 'path/')>
//        Act<A: link('/path/', 'x')>
//        Act<A: put('/path/x')>
//    ...
//    plan #3:
//        Act<A: list('/')>
//        Act<A: get('/path/x')>
//        Act<A: list('/path/')>
//        Act<A: link('/', 'path/')>
//        Act<A: link('/path/', 'x')>
//        Act<A: put('/path/x')>
//    ...
```

These orderings are not arbitrary; `Planner` places some some constraints on the
order of actions by constructing a dependency graph. Some constraints are
"trivial", for example a `put()` call must follow the `get()` call that loads
the current document value and version ID into the actor's memory. But other
constraints are applied to try to ensure correct execution, and those
constraints are configurable via the `Config` object so that we can check which
of them are necessary to avoid consistency violations.

As well as permuting the actions of a single client, we can interleave and
permute the actions of multiple clients. For example here we plan for one client
to update document `/x` while another client removes it. The first generated
execution plan has client `A` execute all its actions before `B` does anything,
but other plans interleave the calls in every possible way.

```rs
planner = Planner::new(config.clone());

planner.client("A").update("/x", |_| Some(('a', 1)));
planner.client("B").remove("/x");

for (i, plan) in planner.orderings().enumerate() {
    println!("plan #{}:", i + 1);
    for act in plan {
        println!("    {:?}", act);
    }
}

// -> plan #1:
//        Act<A: list('/')>
//        Act<A: get('/x')>
//        Act<A: link('/', 'x')>
//        Act<A: put('/x')>
//        Act<B: list('/')>
//        Act<B: get('/x')>
//        Act<B: rm('/x')>
//        Act<B: unlink('/', 'x')>
//    ...
//    plan #99:
//        Act<A: get('/x')>
//        Act<A: list('/')>
//        Act<B: get('/x')>
//        Act<B: list('/')>
//        Act<B: rm('/x')>
//        Act<A: link('/', 'x')>
//        Act<B: unlink('/', 'x')>
//        Act<A: put('/x')>
//    ...
```

To check that an execution plan is valid, we create a `Store` in some initial
state defined by each test scenario. We execute each `Act` in the plan by making
some `Actor` perform the given action, and after each one we check the `Store`
for consistency errors. This means we can detect whether a bad state exists at
any point in the execution, so that if a client crashes or a new client starts
executing a workflow, the store will always be in a valid state.

As soon as we find a failing execution for a given scenario, we stop searching
and print the `Store` state and sequence of actions that produced the failure.
Only if every possible execution for a given scenario completes successfully do
we mark that scenario a success.


### Consistency checks

The key consistency requirement for VaultDB is that, following any write, every
existing document path must have a chain of links leading to it in its parent
directories. That is, if a doc exists at `/path/to/x`:

- The directory `/path/to/` must contain the item `x`
- The directory `/path/` must contain the item `to/`
- The directory `/` must contain the item `path/`

If any of these conditions is not met, the store is in an invalid state. The
`Checker::check()` function checks these conditions and returns `Ok(())` if all
of them are met, and otherwise returns an `Err<Vec<String>>` containing error
messages about the unmet consistency conditions.

For example, here we create a store containing a document and all its required
links, and `Checker::check()` returns `Ok(())`:

```rs
let config = Config::new();
let store = RefCell::new(Store::new(config));
let mut checker = Checker::new(&store);

{
    let mut s = store.borrow_mut();

    s.write(Path::new("/"), None, Db::dir_from(&["path/"]));
    s.write(Path::new("/path/"), None, Db::dir_from(&["to/"]));
    s.write(Path::new("/path/to/"), None, Db::dir_from(&["x"]));

    s.write(Path::new("/path/to/x"), None, Db::Doc(('a', 1)));
}

println!("{:?}", checker.check());
// -> Ok(())
```

Here we follow the above setup by removing the `to/` item from the `/path/`
directory, and `Checker::check()` returns an error:

```rs
let config = Config::new();
let store = RefCell::new(Store::new(config));
let mut checker = Checker::new(&store);

{
    let mut s = store.borrow_mut();

    s.write(Path::new("/"), None, Db::dir_from(&["path/"]));
    let rev = s.write(Path::new("/path/"), None, Db::dir_from(&["to/"]));
    s.write(Path::new("/path/to/"), None, Db::dir_from(&["x"]));

    s.write(Path::new("/path/to/x"), None, Db::Doc(('a', 1)));

    s.write(Path::new("/path/"), rev, Db::dir_from(&[]));
}

println!("{:?}", checker.check());
// -> Err(["dir '/path/' does not include name 'to/', required by doc '/path/to/x'"])
```

Here we follow the original setup by removing the `/path/` directory entirely,
and `Checker::check()` returns an error:

```rs
let config = Config::new();
let store = RefCell::new(Store::new(config));
let mut checker = Checker::new(&store);

{
    let mut s = store.borrow_mut();

    s.write(Path::new("/"), None, Db::dir_from(&["path/"]));
    let rev = s.write(Path::new("/path/"), None, Db::dir_from(&["to/"]));
    s.write(Path::new("/path/to/"), None, Db::dir_from(&["x"]));

    s.write(Path::new("/path/to/x"), None, Db::Doc(('a', 1)));

    s.remove(Path::new("/path/"), rev);
}

println!("{:?}", checker.check());
// -> Err(["dir '/path/', required by doc '/path/to/x', is missing"])
```

The test runner keeps checking permutations as long as `Checker::check()`
returns `Ok(())`. If it returns `Err`, the runner stops executing the current
scenario and reports the error.

Since this check runs every time a client performs an action, the `Checker`
employs an internal mechanism to skip checking the `Store` if it has not changed
since the last time it was checked, to save a little time during execution.


### Configuration

Instances of the `Config` type are passed into various objects during execution
to modify their behaviour. `Config::new()` returns the default configuration,
which produces working executions. It supports the following variations against
which all scenarios are tested:

- `config.update(mode)`: The default `update()` implementation executed by
  `Planner` performs all required `get()` and `list()` calls, followed by all
  required `link()` calls (in any order), followed by a `put()`. Setting `mode`
  to `Update::GetBeforePut` modifies this so that `get()` can happen _after_ the
  `link()` calls, but still before `put()`.

  In other words, the default `update()` dependency graph (`mode =
  Update::ReadsBeforeLinks`) looks like this:

        Shard │
              │      ┌─────────────┐                    ┌─────────────────────┐
            A │      │ get('/doc') │                    │ put('/doc', newDoc) │
              │      └────────────\┘                    └/────────────────────┘
              │                    \                    /
              │                     \                  /
              │   ┌───────────┐     ┌\────────────────/┐
            B │   │ list('/') ------- link('/', 'doc') │
              │   └───────────┘     └──────────────────┘

  While the alternate graph (`mode = Update::GetBeforePut`) looks like this:

        Shard │
              │   ┌─────────────┐                       ┌─────────────────────┐
            A │   │ get('/doc') ------------------------- put('/doc', newDoc) │
              │   └─────────────┘                       └/────────────────────┘
              │                                         /
              │                                        /
              │   ┌───────────┐     ┌─────────────────/┐
            B │   │ list('/') ------- link('/', 'doc') │
              │   └───────────┘     └──────────────────┘

- `config.remove(mode)`: The default `remove()` implementation performs the
  necessary `get()` and `list()` calls, followed by an `rm()` of the requested
  document, followed by a series of `unlink()` calls executed sequentially going
  up the tree. That is, if `/path/to/x` is removed, it executes
  `unlink('/path/to/', 'x')`, then `unlink('/path/', 'to/')`, then `unlink('/',
  'path/')`, if the removal renders each of those directories empty.

  If `mode` is set to `Remove::UnlinkParallel`, these `unlink()` calls are
  executed not sequentially but in parallel, allowing them to execute in any
  order, just like the `link()` calls in an `update()`.

- `config.skip_links(mode)`: In the default configuration, actors will always
  perform a `write()` to the store when a `link()` is done, even if the new item
  already exists in the directory and this would leave the store's values
  unchanged. This causes the directory's version ID to be bumped, which affects
  concurrent writes by other clients.

  If `mode` is set to `true`, then `link()` will not perform a `write()` if the
  directory already includes the required item, so the store's version ID for
  that directory will be unchanged.

- `config.store(mode)`: This affects how a `Store` handles writes to a key that
  does not exist, or has been deleted. The following modes are available:

  - `Cas::Strict`: a `read()` for a deleted key returns `Some((rev, None))`, and
    writes to this key must include `rev` in order to be accepted.

  - `Cas::NoRev`: a `read()` for a deleted key returns `None`, and writes to
    this key must have a version ID of `None` in order to be accepted.

  - `Cas::MatchRev`: a mix of the first two options; reads return `Some((rev,
    None))`, and writes must have a version ID of _either_ `None` or
    `Some(rev)`.

  - `Cas::Lax`: the version ID is not checked for writes to keys that do not
    exist or have been deleted.


## Findings

- Some tests fail when using `Update::GetBeforePut`, indicating that during an
  `update()` call, the `get()` operation must happen _before_ any of the
  `link()` calls.

- Some tests fail when using `Remove::UnlinkParallel`, indicating that it's
  important that `unlink()` calls are performed sequentially and that the order
  is important. The tests work when items are unlinked starting with the most
  deeply nested directory.

- Some tests fail when actors are allowed to skip writes for `link()` calls that
  do not change a directory's contents. This is because performing this write
  changes the directory's version ID in our `Store` (which uses sequential
  integers as versions) and therefore prevents other actors performing
  conflicting writes.

  This shows that clients must be able to indicate they have "touched" a
  directory even if its contents have not changed. On stores that change the
  version ID on every write, this happens implicitly. But if a store uses only
  content hashing for version IDs, then updating a key with identical content
  will not change its version and will not prevent conflicting writes. This
  amounts to observing that such stores are vulnerable to the ABA problem.

  Therefore, to guard against stores that use content hashing for versioning,
  the storage model _must_ change something about the stored data even if its
  semantic payload is unchanged. This could be accomplished by storing a counter
  inside each value/shard that is incremented on each write.

- Some tests fail when the store mode is `Lax`, but the modes `Strict`, `NoRev`
  and `MatchRev` pass all tests. This indicates that requiring _either_ a
  matching version ID or no version ID when writing to deleted keys is
  sufficient for a store to support our consistency requirements. Storage
  backends should be checked for their behaviour when writing to deleted keys
  and reading the value following a deletion.

  It may be possible to cope with stores that implement `Lax` mode by
  representing deleting inside our data model rather than by actually deleting a
  key. In our sharding model this could be as simple as writing a shard
  containing no items, rather than deleting a shard that is empty, so that all
  writes obey strict compare-and-swap semantics.
