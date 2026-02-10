pub const RUST_SNIPPET: &str = r#"// Rust: structs, traits, macros, lifetimes
use std::collections::HashMap;

const MAX_SIZE: usize = 256;

#[derive(Debug, Clone)]
struct Config<'a> {
    name: &'a str,
    count: u32,
}

macro_rules! log_info {
    ($msg:expr) => { println!("[INFO] {}", $msg) };
}

fn process<'a>(config: &Config<'a>) -> bool {
    let items: HashMap<String, i64> = HashMap::new();
    let pattern = regex::Regex::new(r"\d+").unwrap();
    log_info!(config.name);
    println!("{}: {}", config.name, items.len());
    pattern.is_match(config.name)
}

fn main() {
    let cfg = Config { name: "test", count: 42 };
    let _ok = process(&cfg);
}"#;

pub const PYTHON_SNIPPET: &str = r#"# Python: classes, decorators, f-strings
import re
from typing import Dict, List, Optional

MAX_SIZE: int = 256

class Config:
    """Configuration holder."""
    def __init__(self, name: str, count: int):
        self.name = name
        self.count = count

@staticmethod
def process(config: Config) -> bool:
    items: Dict[str, int] = {}
    pattern = re.compile(r"\d+")
    print(f"{config.name}: {len(items)}")
    return bool(pattern.match(config.name))

if __name__ == "__main__":
    cfg = Config(name="test", count=42)
    ok = process(cfg)
"#;

pub const GO_SNIPPET: &str = r#"// Go: structs, interfaces, goroutines
package main

import (
    "fmt"
    "regexp"
)

const MaxSize = 256

type Processor interface {
    Process() bool
}

type Config struct {
    Name  string
    Count int
}

func (c *Config) Process() bool {
    items := make(map[string]int64)
    pattern := regexp.MustCompile(`\d+`)
    fmt.Printf("%s: %d\n", c.Name, len(items))
    return pattern.MatchString(c.Name)
}

func main() {
    cfg := &Config{Name: "test", Count: 42}
    go func() { _ = cfg.Process() }()
}
"#;

pub const JS_SNIPPET: &str = r#"// JavaScript: classes, async/await, regex
import { readFile } from "fs/promises";

const MAX_SIZE = 256;

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
    const data = await readFile("config.json");
    const cfg = new Config("test", 42);
    const ok = process(cfg);
}
"#;
pub const TS_SNIPPET: &str = r#"// TypeScript: interfaces, generics, async
import { EventEmitter } from "events";

const MAX_SIZE: number = 256;

interface Config {
    name: string;
    count: number;
}

type Result<T> = { ok: true; value: T } | { ok: false };

function process(config: Config): boolean {
    const items = new Map<string, number>();
    const pattern = /\d+/g;
    console.log(`${config.name}: ${items.size}`);
    return pattern.test(config.name);
}

async function main(): Promise<void> {
    const cfg: Config = { name: "test", count: 42 };
    const ok = process(cfg);
}
"#;

pub const C_SNIPPET: &str = r#"/* C: structs, pointers, macros */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_SIZE 256
#define LOG(msg) printf("[INFO] %s\n", (msg))

typedef struct {
    const char *name;
    unsigned int count;
} Config;

static int process(const Config *cfg) {
    char buffer[MAX_SIZE];
    snprintf(buffer, sizeof(buffer), "%s: %u",
             cfg->name, cfg->count);
    LOG(buffer);
    return cfg->count > 0 ? 1 : 0;
}

int main(void) {
    Config cfg = { .name = "test", .count = 42 };
    return process(&cfg);
}
"#;

pub const CPP_SNIPPET: &str = r#"// C++: classes, templates, namespaces
#include <iostream>
#include <string>
#include <map>

namespace app {

constexpr int MAX_SIZE = 256;

template<typename T>
class Config {
public:
    std::string name;
    T count;
    Config(std::string n, T c) : name(n), count(c) {}
};

bool process(const Config<int>& cfg) {
    std::map<std::string, int> items;
    auto label = cfg.name + ": " + std::to_string(cfg.count);
    std::cout << label << std::endl;
    return !items.empty();
}

} // namespace app

int main() {
    app::Config<int> cfg("test", 42);
    return app::process(cfg) ? 0 : 1;
}
"#;

pub const JAVA_SNIPPET: &str = r#"// Java: annotations, generics, interfaces
package com.example;

import java.util.HashMap;
import java.util.Map;
import java.util.regex.Pattern;

public class Config {
    private static final int MAX_SIZE = 256;
    private final String name;
    private final int count;

    public Config(String name, int count) {
        this.name = name;
        this.count = count;
    }

    @Override
    public String toString() {
        return name + ": " + count;
    }

    public static boolean process(Config cfg) {
        Map<String, Integer> items = new HashMap<>();
        Pattern pattern = Pattern.compile("\\d+");
        System.out.println(cfg.toString());
        return pattern.matcher(cfg.name).find();
    }

    public static void main(String[] args) {
        Config cfg = new Config("test", 42);
        boolean ok = process(cfg);
    }
}
"#;
