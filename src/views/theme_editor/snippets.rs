pub const RUST_SNIPPET: &str = r#"// A sample Rust program
use std::collections::HashMap;

const MAX_SIZE: usize = 100;

#[derive(Debug, Clone)]
struct Config {
    name: String,
    count: u32,
}

fn process(config: &Config) -> bool {
    let items: HashMap<String, i64> = HashMap::new();
    let pattern = regex::Regex::new(r"\d+").unwrap();
    println!("{}: {}", config.name, items.len());
    pattern.is_match(&config.name)
}

fn main() {
    let cfg = Config {
        name: "test".to_string(),
        count: 42,
    };
    let _ok = process(&cfg);
}"#;

pub const PYTHON_SNIPPET: &str = r#"# A sample Python program
import re
from typing import Dict, List

MAX_SIZE: int = 100

class Config:
    """Configuration holder."""
    def __init__(self, name: str, count: int):
        self.name = name
        self.count = count

def process(config: Config) -> bool:
    items: Dict[str, int] = {}
    pattern = re.compile(r"\d+")
    print(f"{config.name}: {len(items)}")
    return bool(pattern.match(config.name))

if __name__ == "__main__":
    cfg = Config(name="test", count=42)
    ok = process(cfg)
"#;

pub const GO_SNIPPET: &str = r#"// A sample Go program
package main

import (
    "fmt"
    "regexp"
)

const MaxSize = 100

type Config struct {
    Name  string
    Count int
}

func process(cfg *Config) bool {
    items := make(map[string]int64)
    pattern := regexp.MustCompile(`\d+`)
    fmt.Printf("%s: %d\n", cfg.Name, len(items))
    return pattern.MatchString(cfg.Name)
}

func main() {
    cfg := &Config{Name: "test", Count: 42}
    _ = process(cfg)
}
"#;

pub const JS_SNIPPET: &str = r#"// A sample JavaScript program
import { readFile } from "fs/promises";

const MAX_SIZE = 100;

class Config {
    constructor(name, count) {
        this.name = name;
        this.count = count;
    }
}

function process(config) {
    const items = new Map();
    const pattern = /\d+/g;
    console.log(`${config.name}: ${items.size}`);
    return pattern.test(config.name);
}

async function main() {
    const cfg = new Config("test", 42);
    const ok = process(cfg);
}
"#;
