# Scaffold

Esta herramienta permite gestionar plantillas desde la línea de comandos. Es simple, versátil y veloz.

[README en inglés](./README.md)

## Instalación

```sh
cargo install scaffold-cli
```

## Variable de entorno

El programa utiliza únicamente `SCAFFOLDS` como variable de entorno, la cual define la ruta donde se almacenan las plantillas.

```sh
export SCAFFOLDS="$HOME/scaffolds"
```

## Comandos

- `init`: despliega una plantilla. Si no se especifica el destino, se utiliza el directorio en donde se ejecuta el comando.

```sh
scaffold init template [destino]
```

- `list`: lista las plantillas disponibles.

```sh
scaffold list [grupo/plantilla]
```

- `schema`: imprime el contenido de `schema.json`, si está disponible.

```sh
scaffold schema grupo/plantilla
```

- `completion`: genera el script de autocompletado para la shell seleccionada.

```sh
scaffold completion [shell]
```

- `help`: muestra el mensaje de ayuda.

```sh
scaffold help
```

- `version`: muestra la versión del programa.

```sh
scaffold version
```

## Sistema de organización

Para que el programa funcione correctamente, las plantillas deben organizarse en directorios.

El directorio padre (grupo) puede contener uno o varios subdirectorios (plantillas), cada uno con los elementos que deseas utilizar.

```text
$SCAFFOLDS/
├── grupo/
│   ├── plantilla/
│   │   ├── fichero
│   │   └── directorio
│   └── plantilla/
└── grupo/
    ├── plantilla/
    └── plantilla/
```

Por ejemplo, puedes organizar tus `justfiles` para diferentes escenarios.

```text
just/
├── just/
│   └── justfile
└── publish/
    ├── dist/
    │   └── justfile
    └── justfile
```

## Plantilla predeterminada

El programa determina la plantilla predeterminada mediante dos reglas:

1. Cuando es la única plantilla disponible dentro de un grupo.
2. Cuando el nombre de la plantilla coincide con el nombre del grupo.

```text
.
├── ansible/
│   ├── ansible   -> (plantilla predeterminada)
│   ├── database/
│   └── firewall/
├── css/
│   ├── blades/
│   └── css       -> (plantilla predeterminada)
├── just/
│   ├── just      -> (plantilla predeterminada)
│   └── publish/
└── tofu/
    └── forgejo   -> (plantilla predeterminada)
```

En este contexto, los siguientes comandos generan el mismo resultado:

```sh
scaffold init tofu .
scaffold init tofu/forgejo .
```

```sh
scaffold init ansible .
scaffold init ansible/ansible .
```

## Schema

De manera opcional, puedes utilizar `schema.json` para configurar el despliegue de tus plantillas.

Las opciones disponibles son `include`, `exclude`, `variables`, `commands` y `delimiters`.

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

> Las opciones `include` y `exclude` permiten utilizar patrones glob (glob patterns) para seleccionar ficheros y directorios.

### Inclusión de ficheros y directorios

Con `include` puedes seleccionar manualmente los elementos que debe desplegar la plantilla.

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
├── .git           -> (no se incluye)
├── pyproject.toml
├── README.md      -> (no se incluye)
├── schema.json    -> (no se incluye por defecto)
└── src/
    └── tool/
        └── __init__.py
```

### Exclusión de ficheros y directorios

Con `exclude` puedes seleccionar manualmente los elementos que deseas excluir.

**`schema.json`**

```json
{
  "exclude": [
    ".git",
    "README.md"
  ]
}
```

> Esta opción es útil si deseas versionar tus plantillas e incluir su propia documentación.

### Variables

En `schema.json` puedes definir tus propias variables y sus valores se inyectarán en todos los ficheros donde se detecten las variables correspondientes.

Las variables se exponen mediante flags de la línea de comandos al ejecutar `scaffold init`.

Por ejemplo, puedes definir la variable `port`. El programa buscará `{{port}}` en todos los ficheros y realizará la sustitución correspondiente.

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

Un caso de uso más complejo puede ser el siguiente:

```json
{
  "variables": {
    "name": "Nombre de proyecto",
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

### Comandos

Hay casos en los cuales será necesario ejecutar tareas de inicialización al desplegar una plantilla; para ello está disponible la opción `commands`.

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

Las variables que definas previamente pueden ser usadas en los comandos que utilices de la siguiente forma:

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

La plantilla crea la rama `dev` por defecto, pero puedes modificar la variable `branch` desde la línea de comandos.

```sh
scaffold init [plantilla] [directorio] --branch main
```

### Delimitadores personalizados

Por defecto los delimitadores son llaves dobles `{{ variable }}`, pero los puedes modificar si agregas `settings.delimiters`.

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

En este escenario tendrías que usar `#{variable}` en tus ficheros.

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

> Personalmente no me agrada que el software limite al usuario, pero usar delimitadores fijos simplifica el `schema.json` respecto a la versión anterior. Para lidiar con esa contradicción, decidí incluir la opción de definir delimitadores personalizados. `settings` se reserva para posibles personalizaciones en el futuro.

## License

BSD 2-Clause. See [LICENSE](./LICENSE).
