#!/bin/sh
# Copies the launcher UI into the website's live demo. Run after changing src/.
set -e
cd "$(dirname "$0")"
cp ../src/styles.css ../src/main.js demo/
sed 's#<link rel="stylesheet" href="styles.css" />#<link rel="stylesheet" href="styles.css" />\n  <link rel="stylesheet" href="demo.css" />#' ../src/index.html > demo/index.html
