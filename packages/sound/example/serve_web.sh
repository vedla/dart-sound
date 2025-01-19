#!/usr/bin/env bash
# Builds the WebAudio example to JS and serves it on http://localhost:8080.
# Open that URL in Chrome and click "Play 440 Hz" (browsers require a user
# gesture before audio can start).
set -euo pipefail
dir="$(cd "$(dirname "$0")" && pwd)"
dart compile js "$dir/web_main.dart" -o "$dir/web/main.dart.js"
echo "Serving http://localhost:8080 — open it and click Play (Ctrl-C to stop)."
cd "$dir/web"
exec python3 -m http.server 8080
