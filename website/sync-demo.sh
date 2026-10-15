#!/bin/sh
# Copies the launcher UI into the website's live demo. Run after changing src/.
set -e
cd "$(dirname "$0")"
cp ../src/styles.css ../src/main.js ../src/i18n.js demo/
mkdir -p demo/lang
cp ../src/lang/*.js demo/lang/
v=$(date +%s)
sed -e "s#<link rel=\"stylesheet\" href=\"styles.css\" />#<link rel=\"stylesheet\" href=\"styles.css?v=$v\" />\n  <link rel=\"stylesheet\" href=\"demo.css?v=$v\" />#" \
    -e "s#<script src=\"i18n.js\" defer></script>#<script src=\"i18n.js?v=$v\" defer></script>#" \
    -e "s#<script src=\"main.js\" defer></script>#<script src=\"main.js?v=$v\" defer></script>#" \
    ../src/index.html > demo/index.html
