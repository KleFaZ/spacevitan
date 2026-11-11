#!/usr/bin/env python3
"""Baut die Browser-Ansicht mit eingebetteten Partien.

    python3 viewer/build.py AUSGABE.html [partie.json ...] [--wasm spacevitan.wasm] [--standalone]

Ohne --standalone entsteht ein Seitenfragment (so wird es als Artefakt veroeffentlicht).
Mit --standalone bekommt die Datei einen vollstaendigen HTML-Rahmen und laesst sich direkt
im Browser oeffnen.

Partien erzeugt die CLI:  spacevitan --seed 1 --export partie.json
Den Spielkern (--wasm) baut die CI in den Branch web-build:
    git fetch origin +web-build:refs/remotes/origin/web-build && git show origin/web-build:spacevitan.wasm > spacevitan.wasm
Ohne --wasm funktioniert nur die Wiedergabe, nicht das Spielen.
"""

import base64
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
PLACEHOLDER = '<script type="application/json" id="samples">[]</script>'
WASM_PLACEHOLDER = '<script type="application/octet-stream" id="wasm"></script>'


def main(argv):
    standalone = "--standalone" in argv
    args = [a for a in argv if a != "--standalone"]
    wasm = None
    if "--wasm" in args:
        i = args.index("--wasm")
        if i + 1 >= len(args):
            print("--wasm braucht eine Datei", file=sys.stderr)
            return 2
        wasm = pathlib.Path(args[i + 1]).read_bytes()
        if wasm[:4] != b"\0asm":
            print(f"{args[i + 1]} ist keine WebAssembly-Datei", file=sys.stderr)
            return 1
        del args[i : i + 2]
    if len(args) < 1:
        print(__doc__)
        return 2
    out, inputs = pathlib.Path(args[0]), args[1:]

    replays = []
    for path in inputs:
        data = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
        if data.get("format") != "spacevitan-replay":
            print(f"{path}: keine Spacevitan-Partie", file=sys.stderr)
            return 1
        replays.append(data)

    template = (HERE / "index.html").read_text(encoding="utf-8")
    if template.count(PLACEHOLDER) != 1:
        print("Platzhalter fuer die Beispielpartien fehlt in index.html", file=sys.stderr)
        return 1
    payload = json.dumps(replays, separators=(",", ":"), ensure_ascii=False).replace("</", "<\\/")
    page = template.replace(PLACEHOLDER, f'<script type="application/json" id="samples">{payload}</script>')
    if wasm is not None:
        if page.count(WASM_PLACEHOLDER) != 1:
            print("Platzhalter fuer den Spielkern fehlt in index.html", file=sys.stderr)
            return 1
        encoded = base64.b64encode(wasm).decode("ascii")
        page = page.replace(WASM_PLACEHOLDER, f'<script type="application/octet-stream" id="wasm">{encoded}</script>')

    if standalone:
        page = (
            '<!doctype html>\n<html lang="de">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
            "</head>\n<body>\n" + page + "\n</body>\n</html>\n"
        )
    out.write_text(page, encoding="utf-8")
    kern = "mit Spielkern" if wasm is not None else "ohne Spielkern"
    print(f"{out}: {len(replays)} Partien, {kern}, {len(page) // 1024} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
