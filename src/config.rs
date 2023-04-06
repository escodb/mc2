#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    ReadsBeforeLinks,
    GetBeforePut,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Remove {
    UnlinkReverseSequential,
    UnlinkParallel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cas {
    Strict,
    LaxDelete,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub update: Update,
    pub remove: Remove,
    pub skip_links: bool,
    pub store: Cas,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            update: Update::ReadsBeforeLinks,
            remove: Remove::UnlinkReverseSequential,
            skip_links: false,
            store: Cas::Strict,
        }
    }
}

impl Config {
    pub fn new() -> Config {
        Config::default()
    }

    pub fn update(mut self, mode: Update) -> Config {
        self.update = mode;
        self
    }

    pub fn remove(mut self, mode: Remove) -> Config {
        self.remove = mode;
        self
    }

    pub fn skip_links(mut self, mode: bool) -> Config {
        self.skip_links = mode;
        self
    }

    pub fn store(mut self, mode: Cas) -> Config {
        self.store = mode;
        self
    }
}
