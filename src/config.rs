#![allow(dead_code)]

#[derive(Clone, PartialEq)]
pub enum Update {
    ReadsBeforeLinks,
    GetBeforePut,
}

#[derive(Clone)]
pub struct Config {
    pub update: Update,
    pub skip_links: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            update: Update::ReadsBeforeLinks,
            skip_links: false,
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

    pub fn skip_links(mut self, mode: bool) -> Config {
        self.skip_links = mode;
        self
    }
}
