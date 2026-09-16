#!/usr/bin/env python3
"""Normalise COMMAND DOCS + COMMAND into the pinned catalog snapshot schema (V-F01).

Two things matter here and both are about review, not convenience:

* **Determinism.** Keys are sorted, commands are sorted, and only fields the compiler
  actually reads are kept. A snapshot refresh must therefore produce a diff a human can
  read; an unreviewable 180 KB reshuffle is how a wrong effect flag slips in.
* **Integrity.** The snapshot is fingerprinted by ``xtask catalog-stamp``, which writes a
  blake3 digest of the exact file bytes to ``<file>.blake3``. The loader verifies it, so
  hand-patching a command to ``readonly`` cannot pass silently. The hash is computed in Rust
  rather than here so that the whole project has exactly one hash implementation (the same
  one that hashes approvals, ADR-009).
"""

import json
import os
import datetime

TMP = os.environ["TMP"]
OUT = os.environ["OUT"]

with open(f"{TMP}/docs.json", encoding="utf-8") as f:
    DOCS = json.load(f)
with open(f"{TMP}/info.json", encoding="utf-8") as f:
    INFO = json.load(f)

# Fields of an argument node the compiler reads. Anything else is dropped so that
# cosmetic upstream churn does not show up as a snapshot diff.
ARG_KEEP = ("name", "type", "token", "display_text", "since", "key_spec_index")


def norm_arg(a):
    out = {k: a[k] for k in ARG_KEEP if k in a}
    flags = a.get("flags") or []
    # flags is a list of pure tokens: optional / multiple / multiple_token
    for f in ("optional", "multiple", "multiple_token"):
        if f in flags:
            out[f] = True
    if "arguments" in a:
        out["arguments"] = [norm_arg(x) for x in a["arguments"]]
    return out


def norm_info(entry, container=None):
    """One COMMAND reply row -> flat dict. Rows are positional arrays."""
    name = entry[0]
    rows = {
        "name": name.upper(),
        "arity": entry[1],
        "flags": sorted(entry[2] or []),
        "first_key": entry[3],
        "last_key": entry[4],
        "key_step": entry[5],
        "acl": sorted(entry[6] or []),
    }
    if container:
        rows["container"] = container
    subs = entry[9] if len(entry) > 9 else []
    return rows, (subs or [])


info_by_name = {}
for entry in INFO:
    row, subs = norm_info(entry)
    info_by_name[row["name"]] = row
    for s in subs:
        srow, _ = norm_info(s)
        # subcommands come back as "client|list"
        info_by_name[srow["name"]] = srow


def walk_docs(name, doc, container=None):
    out = []
    # DOCS already keys subcommands by their full "client|list" name, so prefixing the
    # container again would produce CLIENT|CLIENT|LIST.
    full = name.upper() if "|" in name else (f"{container}|{name}" if container else name).upper()
    rec = {"name": full}
    if container:
        rec["container"] = container.upper()
    for k in ("summary", "since", "group", "complexity"):
        if k in doc:
            rec[k] = doc[k]
    if "arguments" in doc:
        rec["arguments"] = [norm_arg(a) for a in doc["arguments"]]
    inf = info_by_name.get(full)
    if inf:
        for k in ("arity", "flags", "first_key", "last_key", "key_step", "acl"):
            if k in inf:
                rec[k] = inf[k]
    out.append(rec)
    for sub_name, sub_doc in (doc.get("subcommands") or {}).items():
        out.extend(walk_docs(sub_name, sub_doc, container=full))
    return out


commands = []
for name, doc in DOCS.items():
    commands.extend(walk_docs(name, doc))
commands.sort(key=lambda c: c["name"])

snapshot = {
    "schema": 1,
    "provenance": {
        "family": os.environ["FAMILY"],
        "line": os.environ["LINE"],
        "server_version": os.environ["VERSION"].strip(),
        "image": os.environ["IMAGE"],
        "captured_utc": datetime.datetime.now(datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "source": ["COMMAND DOCS", "COMMAND"],
    },
    "commands": commands,
}

with open(OUT, "w", encoding="utf-8") as f:
    json.dump(snapshot, f, indent=1, sort_keys=True, ensure_ascii=False)
    f.write("\n")
