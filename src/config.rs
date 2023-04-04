#![allow(dead_code)]

pub struct Config {
    pub skip_links: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { skip_links: false }
    }
}

impl Config {
    pub fn new() -> Config {
        Config::default()
    }

    pub fn skip_links(mut self, mode: bool) -> Config {
        self.skip_links = mode;
        self
    }
}
