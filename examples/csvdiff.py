#!/usr/bin/env python3
"""An annotated data diff for sideview's sv-csv (V4.sv's cell-marks bullet).

usage: csvdiff.py [--style marks|inline|split] OLD.csv NEW.csv KEYCOL > diff.csv

Compares two CSVs by key column, the comparison the skill says to compute
yourself. Three output shapes over the same mark machinery — the marks don't
care what the cell text says:

  marks   (default) new values; changed cells carry `_sv_mark_<col>` = mod,
          rows carry `_sv_row` add/del/mod.
  inline  like marks, but a changed cell reads `old -> new` — the git-style
          inline diff, both values in the cell.
  split   side-by-side: each compared column becomes an old/new pair; where
          changed, the old cell is marked del and the new add, like git's
          side-by-side view.

Unchanged rows pass through unmarked in every style, so the diff reads in
context. Output column order follows NEW.
"""
import csv
import sys


def load(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def main():
    args = sys.argv[1:]
    style = "marks"
    if args[:1] == ["--style"]:
        style = args[1]
        args = args[2:]
    if len(args) != 3 or style not in ("marks", "inline", "split"):
        sys.exit(__doc__.strip())
    old, new = load(args[0]), load(args[1])
    key = args[2]
    cols = list(new[0].keys())
    marks = [c for c in cols if c != key]
    old_by_key = {r[key]: r for r in old}
    new_keys = {r[key] for r in new}
    out = csv.writer(sys.stdout)

    if style == "split":
        # key, then old/new pairs; the pair headers are what the marks name.
        pairs = [(f"{c} (old)", f"{c} (new)") for c in marks]
        header = [key] + [h for pair in pairs for h in pair]
        dir_cols = [f"_sv_mark_{h}" for pair in pairs for h in pair]
        out.writerow(["_sv_row"] + dir_cols + header)
        def emit(kind, o, n):
            row_marks, cells = [], []
            for c in marks:
                ov = o.get(c, "") if o else ""
                nv = n.get(c, "") if n else ""
                changed = o is not None and n is not None and ov != nv
                row_marks += ["del" if changed else "", "add" if changed else ""]
                cells += [ov, nv]
            keyval = (n or o)[key]
            out.writerow([kind] + row_marks + [keyval] + cells)
        for r in new:
            o = old_by_key.get(r[key])
            if o is None:
                emit("add", None, r)
            else:
                emit("mod" if any(o.get(c, "") != r.get(c, "") for c in marks) else "", o, r)
        for r in old:
            if r[key] not in new_keys:
                emit("del", r, None)
        return

    out.writerow(["_sv_row"] + [f"_sv_mark_{c}" for c in marks] + cols)

    def emit(kind, r, changed=frozenset(), old_row=None):
        cells = []
        for c in cols:
            v = r.get(c, "")
            if style == "inline" and c in changed and old_row is not None:
                v = f"{old_row.get(c, '')} -> {v}"
            cells.append(v)
        out.writerow([kind] + [("mod" if c in changed else "") for c in marks] + cells)

    for r in new:
        o = old_by_key.get(r[key])
        if o is None:
            emit("add", r)
        else:
            changed = {c for c in cols if o.get(c, "") != r.get(c, "")}
            emit("mod" if changed else "", r, changed, o)
    for r in old:
        if r[key] not in new_keys:
            emit("del", r)


if __name__ == "__main__":
    main()
