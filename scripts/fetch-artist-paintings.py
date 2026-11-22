# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "pillow>=10.0",
#   "piexif>=1.1.3",
#   "requests>=2.31",
# ]
# ///
"""Fetch high-resolution public-domain / CC0 paintings for an artist from Wikidata + Wikimedia Commons,
and write Windows Explorer EXIF Title, Author, Comments, and delimited Tags (XPKeywords).
"""
from __future__ import annotations

import argparse
import io
import re
import time
import urllib.parse
from pathlib import Path

import piexif
import requests
from PIL import Image

UA = "lenscraft-fetch/0.3 (https://github.com/sandraschi/lenscraft)"


def resolve_artist_qid(name: str) -> tuple[str, str]:
    if name.startswith("Q") and name[1:].isdigit():
        return name, name
    r = requests.get(
        "https://www.wikidata.org/w/api.php",
        params={"action": "wbsearchentities", "search": name, "language": "en", "limit": 3, "format": "json"},
        headers={"User-Agent": UA},
        timeout=15,
    )
    r.raise_for_status()
    hits = r.json().get("search", [])
    if not hits:
        raise SystemExit(f"Artist not found on Wikidata: {name}")
    return hits[0]["id"], hits[0].get("label", name)


def fetch_artist_works(qid: str, limit: int) -> list[dict]:
    sparql = f"""
    SELECT ?item ?itemLabel ?collectionLabel ?locationLabel ?genreLabel ?materialLabel ?inception ?image WHERE {{
      ?item wdt:P170 wd:{qid} .
      ?item wdt:P31 wd:Q3305213 .
      ?item wdt:P18 ?image .
      OPTIONAL {{ ?item wdt:P195 ?collection . }}
      OPTIONAL {{ ?item wdt:P276 ?location . }}
      OPTIONAL {{ ?item wdt:P136 ?genre . }}
      OPTIONAL {{ ?item wdt:P186 ?material . }}
      OPTIONAL {{ ?item wdt:P571 ?inception . }}
      SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,de,it". }}
    }} LIMIT {limit}
    """
    r = requests.get(
        "https://query.wikidata.org/sparql",
        params={"format": "json", "query": sparql},
        headers={"User-Agent": UA},
        timeout=30,
    )
    r.raise_for_status()
    rows = r.json().get("results", {}).get("bindings", [])
    seen = set()
    works = []
    for b in rows:
        item_qid = b.get("item", {}).get("value", "").rsplit("/", 1)[-1]
        if not item_qid or item_qid in seen:
            continue
        seen.add(item_qid)
        title = b.get("itemLabel", {}).get("value", "").strip()
        if not title or title == item_qid:
            continue
        inc = b.get("inception", {}).get("value", "")[:4]
        works.append(
            {
                "qid": item_qid,
                "title": title,
                "collection": b.get("collectionLabel", {}).get("value", ""),
                "location": b.get("locationLabel", {}).get("value", ""),
                "genre": b.get("genreLabel", {}).get("value", ""),
                "material": b.get("materialLabel", {}).get("value", ""),
                "year": inc if inc.isdigit() else "",
                "image": b.get("image", {}).get("value", ""),
            }
        )
    return works


def safe_filename(author: str, title: str, year: str) -> str:
    clean = re.sub(r'[\\/:*?"<>|]', "", f"{author} - {title}").strip()
    if year:
        clean = f"{clean} ({year})"
    return clean[:160] + ".jpg"


def write_exif(jpg_path: Path, title: str, author: str, comment: str, tags: list[str]) -> None:
    try:
        exif_dict = piexif.load(str(jpg_path))
    except Exception:
        exif_dict = {"0th": {}, "Exif": {}, "GPS": {}, "1st": {}, "thumbnail": None}
    z = exif_dict.setdefault("0th", {})
    z[piexif.ImageIFD.ImageDescription] = title.encode("utf-8", errors="replace")
    z[piexif.ImageIFD.Artist] = author.encode("utf-8", errors="replace")
    z[0x9C9B] = (title + "\x00").encode("utf-16le")
    z[0x9C9C] = (comment + "\x00").encode("utf-16le")
    z[0x9C9D] = (author + "\x00").encode("utf-16le")
    tag_str = "; ".join(t.strip() for t in tags if t and t.strip())
    if tag_str:
        z[0x9C9E] = (tag_str + "\x00").encode("utf-16le")
    piexif.insert(piexif.dump(exif_dict), str(jpg_path))


def main() -> int:
    ap = argparse.ArgumentParser(description="Fetch high-res PD paintings for an artist")
    ap.add_argument("artist", help="Artist name or Wikidata QID (e.g. Parmigianino)")
    ap.add_argument("out_dir", type=Path, help="Target folder")
    ap.add_argument("--limit", type=int, default=10, help="Max paintings to download (<= 10 per run)")
    ap.add_argument("--min-width", type=int, default=1200, help="Minimum width in pixels")
    args = ap.parse_args()

    qid, artist_label = resolve_artist_qid(args.artist)
    works = fetch_artist_works(qid, max(args.limit * 2, 20))
    args.out_dir.mkdir(parents=True, exist_ok=True)

    saved = 0
    for w in works:
        if saved >= min(args.limit, 10):
            break
        out_path = args.out_dir / safe_filename(artist_label, w["title"], w["year"])
        if out_path.exists():
            continue
        img_url = w["image"]
        if "?" not in img_url:
            img_url = f"{img_url}?width=2000"
        try:
            resp = requests.get(img_url, headers={"User-Agent": UA}, timeout=25)
            if not resp.ok:
                continue
            im = Image.open(io.BytesIO(resp.content))
            if max(im.size) < args.min_width:
                continue
            if im.mode != "RGB":
                im = im.convert("RGB")
            im.save(out_path, format="JPEG", quality=93)
            parts = []
            if w["collection"]:
                parts.append(f"Museum: {w['collection']}")
            if w["location"]:
                parts.append(f"Location: {w['location']}")
            if w["year"]:
                parts.append(f"Date: {w['year']}")
            parts.append(f"Wikidata: {w['qid']}")
            comment = " | ".join(parts)
            tags = [w["collection"], w["genre"], w["material"], w["location"], artist_label]
            write_exif(out_path, w["title"], artist_label, comment, tags)
            saved += 1
            print(f"[SAVED {im.size[0]}x{im.size[1]}] {out_path.name}")
            time.sleep(0.8)
        except Exception as exc:
            print(f"[SKIP] {w['title']}: {exc}")
    print(f"Downloaded {saved} paintings for {artist_label} ({qid}) into {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
