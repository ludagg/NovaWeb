# NovaWeb

NovaWeb is a programming language implemented in Rust.

## Features
- Scoped variables and assignments
- Standard control flow: `if/else`, `while`, `for`
- First-class functions
- Built-in HTML template engine with variable injection
- Basic types: Int, Float, Bool, String, List, Map

## Installation
Ensure you have Rust and Cargo installed.

```bash
cargo build --release
```

## Usage
To run a NovaWeb script:

```bash
./target/release/novaw run <path_to_script>
```

## Example
Check the `examples/` directory for sample scripts.

```novaw
let user = "Alice";
let ctx = { "user": user };
let template = "Hello, {{ user }}!";
print(render(template, ctx));
```
