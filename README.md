# Scaffold

This tool allows you to manage templates from the command line. It is simple, versatile, and fast.

[README in Spanish](./README.es.md)

## Installation

```sh
cargo install scaffold-cli
```

## Environment variable

The program uses only `SCAFFOLDS` as an environment variable, which defines the path where templates are stored.

```sh
export SCAFFOLDS="$HOME/scaffolds"
```

## Commands

- `init`: deploys a template. If no destination is specified, the directory where the command is executed is used.

```sh
scaffold init template [destination]
```

- `list`: lists the available templates.

```sh
scaffold list [group/template]
```

- `schema`: prints the contents of `schema.json`, if available.

```sh
scaffold schema group/template
```

- `completion`: generates the autocompletion script for the selected shell.

```sh
scaffold completion [shell]
```

- `help`: displays the help message.

```sh
scaffold help
```

- `version`: displays the program version.

```sh
scaffold version
```

## Organization system

For the program to work correctly, templates must be organized into directories.

The parent directory (group) can contain one or more subdirectories (templates), each with the elements you want to use.

```text
$SCAFFOLDS/
├── group/
│   ├── template/
│   │   ├── file
│   │   └── directory
│   └── template/
└── group/
    ├── template/
    └── template/
```

For example, you can organize your `justfiles` for different scenarios.

```text
just/
├── just/
│   └── justfile
└── publish/
    ├── dist/
    │   └── justfile
    └── justfile
```

## Default template

The program determines the default template according to two rules:

1. When it is the only template available within a group.
2. When the template name matches the group name.

```text
.
├── ansible/
│   ├── ansible   -> (default template)
│   ├── database/
│   └── firewall/
├── css/
│   ├── blades/
│   └── css       -> (default template)
├── just/
│   ├── just      -> (default template)
│   └── publish/
└── tofu/
    └── forgejo   -> (default template)
```

In this context, the following commands produce the same result:

```sh
scaffold init tofu .
scaffold init tofu/forgejo .
```

```sh
scaffold init ansible .
scaffold init ansible/ansible .
```

## Schema

Optionally, you can use `schema.json` to configure the deployment of your templates.

The available options are `include`, `exclude`, `variables`, `commands`, and `delimiters`.

```text
just/
├── just/
│   ├── justfile
│   └── schema.json
└── publish/
    ├── dist/
    │   └── justfile
    ├── justfile
    └── schema.json
```

> The `include` and `exclude` options allow you to use glob patterns to select files and directories.

### Including files and directories

With `include`, you can manually select the elements that the template should deploy.

**`schema.json`**

```json
{
  "include": [
    "pyproject.toml",
    "src/**/*.py"
  ]
}
```

```text
.
├── .git           -> (not included)
├── pyproject.toml
├── README.md      -> (not included)
├── schema.json    -> (not included by default)
└── src/
    └── tool/
        └── __init__.py
```

### Excluding files and directories

With `exclude`, you can manually select the elements you want to exclude.

**`schema.json`**

```json
{
  "exclude": [
    ".git",
    "README.md"
  ]
}
```

> This option is useful if you want to version your templates and include their own documentation.

### Variables

In `schema.json`, you can define your own variables, and their values will be injected into all files where the corresponding variables are detected.

Variables are exposed through command-line flags when running `scaffold init`.

For example, you can define the `port` variable. The program will search for `{{port}}` in all files and perform the corresponding substitution.

```json
{
  "variables": {
    "port": "8080"
  }
}
```

```sh
scaffold init [template] --port 9000
```

A more complex use case could be the following:

```json
{
  "variables": {
    "name": "Project name",
    "description": "Hello world",
    "port": 8080
  }
}
```

```sh
scaffold init web/laravel ./new-dashboard \
  --name dashboard \
  --description "Hotel reservation dashboard" \
  --port 9000
```

### Commands

There may be cases where initialization tasks need to be executed when deploying a template; for this purpose, the `commands` option is available.

**`schema.json`**

```json
{
  "commands": [
    ["git", "init", "--initial-branch=dev"],
    ["npm", "install"],
    ["direnv", "allow"]
  ]
}
```

Variables defined previously can be used in commands as follows:

**`schema.json`**

```json
{
  "variables": {
    "branch": "dev"
  },
  "commands": [
    ["git", "init", "--initial-branch={{branch}}"]
  ]
}
```

The template creates the `dev` branch by default, but you can modify the `branch` variable from the command line.

```sh
scaffold init [template] [directory] --branch main
```

### Custom delimiters

By default, the delimiters are double braces `{{ variable }}`, but you can change them by adding `settings.delimiters`.

**`schema.json`**

```json
{
  "settings": {
    "delimiters": {
      "open": "#{",
      "close": "}"
    }
  }
}
```

In this scenario, you would need to use `#{variable}` in your files.

**`Cargo.toml`**

```toml
[package]
name = "#{name}"
description = "#{name}"

[[bin]]
name = "#{bin}"
path = "src/main.rs"
```

**`src/main.rs`**

```rust
fn main() {
    println!("#{name}");
}
```

> Personally, I don't like software that limits the user, but using fixed delimiters simplifies `schema.json` compared to the previous version. To deal with this contradiction, I decided to include the option to define custom delimiters. `settings` is reserved for possible customizations in the future.

## License

BSD 2-Clause. See [LICENSE](./LICENSE).

