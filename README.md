# Scaffold

This tool allows you to manage templates from the command line.

[README in Spanish](./README.es.md)

## Installation

```sh
cargo install scaffold-cli
```

## Environment Variable

The program uses only `SCAFFOLDS`, an environment variable that defines the path where templates are stored.

```sh
export SCAFFOLDS="$HOME/scaffolds"
```

## Commands

- `init`: deploys a template. If no destination is specified, the current directory is used.

```sh
scaffold init template [destination]
```

- `list`: lists available templates.

```sh
scaffold list [group/template]
```

- `schema`: prints the content of `schema.json`, if available.

```sh
scaffold schema group/template
```

- `completion`: generates the completion script for the selected shell.

```sh
scaffold completion [bash]
```

- `help`: displays the help message.

```sh
scaffold help
```

- `version`: displays the program version.

```sh
scaffold version
```

## Organization System

For the program to work correctly, templates must be organized in directories.

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

## Default Template

The program determines the default template using two rules:

1. When it's the only template available within a group.
2. When its name matches the group name.

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

Optionally, you can use `schema.json` to configure your template deployments.

Available options are `include`, `exclude`, and `variables`.

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

The `include` and `exclude` options allow you to use glob patterns to select files and directories.

### Include

With `include` you can manually select the elements that the template should deploy.

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

### Exclude

With `exclude` you can manually select the elements you want to exclude.

```json
{
  "exclude": [
    ".git"
  ]
}
```

### Variables

In `schema.json` you can define your own variables and their values will be injected into all files where the corresponding words are detected.

Variables are exposed as command-line flags when running `scaffold init`.

For example, you can define the `port` variable. The program will look for `PORT` in the files and perform the corresponding replacement.

```json
{
  "variables": {
    "port": {
      "PORT": "8080"
    }
  }
}
```

The `port` variable becomes the `--port` flag, while `PORT` is the word that will be replaced in each file.

```sh
scaffold init [template] --port 9000
```

You can use as many variables as you want and freely define the syntax of the words to be replaced.

You can use braces `{}`, underscores `_`, characters like `$` or any other format you prefer.

```json
{
  "variables": {
    "name": {
      "{name}": "Project name"
    },
    "description": {
      "$description": "Hello world"
    },
    "port": {
      "_port": "8080"
    }
  }
}
```

```sh
scaffold init web/laravel ./new-dashboard \
  --name dashboard \
  --description "Hotel reservation dashboard" \
  --port 9000
```

## License

BSD 2-Clause. See [LICENSE](./LICENSE).
