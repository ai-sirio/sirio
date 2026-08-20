#!/usr/bin/env python3
"""Read the project table two ways, to separate 'nothing was written' from
'I read a stale snapshot'.

  live <db> <tag>   open the db in place, read-only, sidecars present
  bare <db> <tag>   copy ONLY the .sqlite to an empty dir -- no -wal, no -shm --
                    and read that, which is the protocol under test

Read-only URIs throughout: opening read-write triggers a checkpoint, which
silently repairs the very staleness we are trying to detect.
"""
import os
import shutil
import sqlite3
import sys
import tempfile

COLS = "id, name, root_path, color_hex, display_name, icon_kind, icon_value"


def read(path):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return con.execute(f"SELECT {COLS} FROM project ORDER BY root_path").fetchall()
    finally:
        con.close()


def main():
    mode, db, tag = sys.argv[1], sys.argv[2], sys.argv[3]
    if mode == "bare":
        tmp = tempfile.mkdtemp(prefix="bareread-")
        target = os.path.join(tmp, "copy.sqlite")
        shutil.copyfile(db, target)  # deliberately NOT the -wal/-shm neighbours
        sidecars = [
            os.path.basename(db) + s
            for s in ("-wal", "-shm")
            if os.path.exists(db + s)
        ]
        rows = read(target)
        note = f"sidecars left behind: {sidecars or 'none present'}"
    else:
        rows = read(db)
        wal = db + "-wal"
        size = os.path.getsize(wal) if os.path.exists(wal) else None
        note = f"-wal {'absent' if size is None else str(size) + ' bytes'}"

    print(f"### {tag}  [{mode}]  {note}")
    print(f"### main .sqlite mtime {os.path.getmtime(db):.3f}  size {os.path.getsize(db)}")
    print(f"### {COLS}")
    if not rows:
        print("    (no rows)")
    for r in rows:
        print("   ", r)
    print(f"### projects: {len(rows)}")
    sys.stdout.flush()


main()
