#!/bin/sh
# Runs Graft as PUID:PGID (default 1000:1000) and hands it the data directory and
# database, so the host user with those ids owns them and can read and back them up.
set -eu

if [ "$(id -u)" != 0 ]; then
    exec "$@"
fi

PUID=${PUID:-1000}
PGID=${PGID:-1000}
for id in "$PUID" "$PGID"; do
    case $id in
        '' | *[!0-9]*)
            echo "PUID and PGID must be numeric ids, got '$id'" >&2
            exit 1
            ;;
    esac
done

chown "$PUID:$PGID" "$GRAFT_DATA_DIR"
find "$GRAFT_DATA_DIR" -maxdepth 1 \( -name graft.db -o -name graft.db-wal -o -name graft.db-shm \) \
    -exec chown "$PUID:$PGID" {} +
exec su-exec "$PUID:$PGID" "$@"
