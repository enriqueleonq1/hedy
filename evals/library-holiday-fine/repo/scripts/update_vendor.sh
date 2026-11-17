#!/bin/sh
# Refreshes the vendored copy of textable. Needs network access; run it by hand.
set -eu
version="${1:-0.4.1}"
dest="$(dirname "$0")/../library/vendor/textable.py"
echo "Downloading textable $version into $dest"
python3 -m pip download --no-deps --no-binary :all: "textable==$version" -d /tmp/textable
tar -xzf "/tmp/textable/textable-$version.tar.gz" -C /tmp/textable
cp "/tmp/textable/textable-$version/textable.py" "$dest"
