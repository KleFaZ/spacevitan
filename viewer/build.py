#!/usr/bin/env python3
"""Baut die Browser-Ansicht mit eingebetteten Partien.

    python3 viewer/build.py AUSGABE.html partie1.json [partie2.json ...] [--standalone]

Ohne --standalone entsteht ein Seitenfragment (so wird es als Artefakt veroeffentlicht).
Mit --standalone bekommt die Datei einen vollstaendigen HTML-Rahmen und laesst sich direkt
im Browser oeffnen.

Partien erzeugt die CLI:  spacevitan --seed 1 --export partie.json
"""

import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
PLACEHOLDER = '<script type="application/json" id="samples">[]</script>'


def main(argv):
    standalone = "--standalone" in argv
    args = [a for a in argv if a != "--standalone"]
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

    if standalone:
        page = (
            '<!doctype html>\n<html lang="de">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
            "</head>\n<body>\n" + page + "\n</body>\n</html>\n"
        )
    out.write_text(page, encoding="utf-8")
    print(f"{out}: {len(replays)} Partien, {len(page) // 1024} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
