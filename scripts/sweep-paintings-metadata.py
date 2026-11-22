# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "pillow>=10.0",
#   "piexif>=1.1.3",
#   "requests>=2.31",
# ]
# ///
"""AI metadata sweep for painting JPGs using lenscraft-mcp + Wikidata/Wikipedia grounding.

Writes Windows Explorer-compatible EXIF properties (XPTitle, XPAuthor, XPComment,
ImageDescription, Artist) without recompressing JPEG pixels.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import shutil
import subprocess
from pathlib import Path

import piexif
import requests

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_EXE = REPO_ROOT / "target" / "debug" / "lenscraft-mcp.exe"


def clean_filename_hint(path: Path) -> tuple[str, str]:
    """Extract (author_guess, title_guess) from 'Author - Title (Year).JPG'."""
    stem = path.stem
    folder_author = path.parent.name
    if " - " in stem:
        left, right = stem.split(" - ", 1)
        author = left.strip()
        title = re.sub(r"\s*[\(\[].*?[\)\]]", "", right).strip()
        return author or folder_author, title
    return folder_author, stem


def run_lenscraft_identify(exe: Path, img_path: Path, hint: str) -> dict:
    proc = subprocess.run(
        [str(exe), "identify", str(img_path), "--hint", hint, "--json"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=180,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or proc.stdout.strip())
    return json.loads(proc.stdout)


def run_lenscraft_ground(exe: Path, title: str, creator: str) -> dict | None:
    proc = subprocess.run(
        [str(exe), "ground", title, "--creator", creator, "--json"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=30,
    )
    if proc.returncode != 0:
        return None
    data = json.loads(proc.stdout)
    return data.get("grounding")


def fetch_wikidata_provenance(qid: str) -> tuple[str, str]:
    """Fetch location (P276) and provenance/inception summary for a Wikidata QID."""
    if not qid:
        return "", ""
    url = f"https://www.wikidata.org/w/api.php?action=wbgetentities&ids={qid}&props=claims&format=json"
    try:
        r = requests.get(url, headers={"User-Agent": "lenscraft-sweep/0.3"}, timeout=12)
        r.raise_for_status()
        ent = r.json().get("entities", {}).get(qid, {})
        claims = ent.get("claims", {})
        loc_qids = [
            c.get("mainsnak", {}).get("datavalue", {}).get("value", {}).get("id")
            for c in claims.get("P276", [])[:1]
        ]
        prov_count = len(claims.get("P127", [])) + len(claims.get("P800", []))
        loc = ""
        if loc_qids and loc_qids[0]:
            lr = requests.get(
                f"https://www.wikidata.org/w/api.php?action=wbgetentities&ids={loc_qids[0]}&props=labels&languages=en&format=json",
                headers={"User-Agent": "lenscraft-sweep/0.3"},
                timeout=10,
            )
            if lr.ok:
                loc = (
                    lr.json()
                    .get("entities", {})
                    .get(loc_qids[0], {})
                    .get("labels", {})
                    .get("en", {})
                    .get("value", "")
                )
        prov = f"{prov_count} recorded ownership/notable links" if prov_count else ""
        return loc, prov
    except Exception:
        return "", ""


def ollama_provenance_fallback(title: str, author: str, year_hint: str) -> tuple[str, str, str]:
    """Ask local Ollama for museum, location, and provenance when Wikidata P195 is absent."""
    prompt = (
        f"For the artwork '{title}' by {author} ({year_hint or 'date unknown'}), "
        "return JSON with keys: museum, location (city, country), provenance (1 concise phrase). "
        "If unsure, use 'Unknown'."
    )
    try:
        r = requests.post(
            "http://127.0.0.1:11434/api/chat",
            json={
                "model": "gemma4:12b",
                "stream": False,
                "think": False,
                "format": {
                    "type": "object",
                    "properties": {
                        "museum": {"type": "string"},
                        "location": {"type": "string"},
                        "provenance": {"type": "string"},
                    },
                    "required": ["museum", "location", "provenance"],
                },
                "messages": [{"role": "user", "content": prompt}],
            },
            timeout=30,
        )
        if not r.ok:
            return "", "", ""
        data = json.loads(r.json().get("message", {}).get("content", "{}"))
        return (
            str(data.get("museum") or "").strip(),
            str(data.get("location") or "").strip(),
            str(data.get("provenance") or "").strip(),
        )
    except Exception:
        return "", "", ""


def write_windows_exif(jpg_path: Path, title: str, author: str, comment: str) -> None:
    try:
        exif_dict = piexif.load(str(jpg_path))
    except Exception:
        exif_dict = {"0th": {}, "Exif": {}, "GPS": {}, "1st": {}, "thumbnail": None}

    zeroth = exif_dict.setdefault("0th", {})
    zeroth[piexif.ImageIFD.ImageDescription] = title.encode("utf-8", errors="replace")
    zeroth[piexif.ImageIFD.Artist] = author.encode("utf-8", errors="replace")
    # Windows Explorer Details tab reads XPTitle (0x9C9B), XPAuthor (0x9C9D), XPComment (0x9C9C) in UTF-16LE
    zeroth[0x9C9B] = (title + "\x00").encode("utf-16le")
    zeroth[0x9C9D] = (author + "\x00").encode("utf-16le")
    zeroth[0x9C9C] = (comment + "\x00").encode("utf-16le")

    exif_bytes = piexif.dump(exif_dict)
    piexif.insert(exif_bytes, str(jpg_path))


def main() -> int:
    ap = argparse.ArgumentParser(description="AI metadata sweep for painting JPGs")
    ap.add_argument("folder", type=Path, help="Folder of painting JPGs")
    ap.add_argument("--exe", type=Path, default=DEFAULT_EXE)
    ap.add_argument("--limit", type=int, default=10, help="Max files per batch (<= 10 recommended)")
    ap.add_argument("--apply", action="store_true", help="Write EXIF tags (creates .bak first)")
    ap.add_argument("--report", type=Path, default=Path("sweep_report.json"))
    args = ap.parse_args()

    if not args.folder.is_dir():
        raise SystemExit(f"Not a directory: {args.folder}")
    if not args.exe.exists():
        raise SystemExit(f"lenscraft-mcp.exe not found at {args.exe}; run cargo build -p lenscraft-mcp")

    files = sorted(
        [p for p in args.folder.iterdir() if p.is_file() and p.suffix.lower() in {".jpg", ".jpeg"}]
    )[: max(1, min(args.limit, 10))]

    results = []
    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    for p in files:
        author_guess, title_guess = clean_filename_hint(p)
        hint = f"Folder: {p.parent.name}; Filename title: {title_guess}; Painter: {author_guess}"
        ident = run_lenscraft_identify(args.exe, p, hint)
        top = (ident.get("candidates") or [{}])[0]
        cand_title = top.get("title") or title_guess
        cand_creator = top.get("creator") or author_guess

        ground = run_lenscraft_ground(args.exe, cand_title, cand_creator) or {}
        final_title = ground.get("label") or cand_title
        final_author = ground.get("creator") or cand_creator
        museum = ground.get("collection") or ""
        qid = ground.get("qid") or ""
        loc, prov = fetch_wikidata_provenance(qid)
        if not museum or museum == "Unknown collection":
            year_m = re.search(r"\b(1[3-9]\d{2}|20\d{2})\b", p.stem)
            y_hint = year_m.group(1) if year_m else ""
            m2, l2, p2 = ollama_provenance_fallback(final_title, final_author, y_hint)
            museum = museum or m2 or "Unknown collection"
            loc = loc or l2
            prov = prov or p2
        desc = (ground.get("wikipedia_extract") or ident.get("description") or "").strip()
        # Keep comment concise (<= 500 chars)
        short_desc = " ".join(desc.split())[:320]
        parts = [f"Museum: {museum}"]
        if loc:
            parts.append(f"Location: {loc}")
        if prov:
            parts.append(f"Provenance: {prov}")
        if ground.get("inception"):
            parts.append(f"Date: {ground['inception']}")
        if qid:
            parts.append(f"Wikidata: {qid}")
        if short_desc:
            parts.append(short_desc)
        comment = " | ".join(parts)

        if args.apply:
            bak = p.with_name(f"{p.name}.{stamp}.bak")
            shutil.copy2(p, bak)
            write_windows_exif(p, final_title, final_author, comment)

        row = {
            "file": str(p),
            "title": final_title,
            "author": final_author,
            "comment": comment,
            "applied": args.apply,
        }
        results.append(row)
        print(f"[{'APPLIED' if args.apply else 'DRY-RUN'}] {p.name} -> {final_title} ({final_author})")

    args.report.write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Wrote report to {args.report} ({len(results)} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
