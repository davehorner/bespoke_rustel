"""Create a uniquely named writable score; keep demo templates untouched."""
from pathlib import Path
import argparse
import uuid

ROOT = Path(__file__).resolve().parent


def create_session_score(template, root=ROOT):
    template = Path(template).resolve()
    data = template.read_bytes()
    folder = Path(root).resolve() / "sessions"
    folder.mkdir(parents=True, exist_ok=True)
    name = "".join(c if c.isalnum() or c in "-_" else "_" for c in template.stem)[:60] or "session"
    destination = folder / f"{name}-{uuid.uuid4().hex[:12]}.strudel"
    with destination.open("xb") as score:
        score.write(data)
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("template")
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    print(create_session_score(args.template, args.root))
