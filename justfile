mod dist

set unstable := true

default: watch

publish:
    just  release-info
    just  release-build
    cargo publish
    just clean

release-info:
    just dist::info

release-build:
    just dist::tag
    just dist::build
    just dist::artifacts
    just dist::checksum
    just dist::upload

hint := ""
commit *files="":
    gareth repo commit {{ if hint != "" { "--hint " + quote(hint) } else { "" } }} {{ files }} | git commit {{ files }} -F - --edit

install:
    cargo install --path . --bin scaffold

dump:
    find src -type f -exec sh -c 'for f; do echo "--- $f ---"; cat "$f"; done' sh {} + | larry "tree -I 'docs|target|result'" 

watch task="check":
    cargo watch -c -w src -x {{ task }}

clean:
    rm -rf target
    rm -rf result
