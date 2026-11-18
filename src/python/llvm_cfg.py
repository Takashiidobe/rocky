#!/usr/bin/env python3
"""
llvm_cfg.py - recover and draw a control-flow graph (diagram + CSV) from textual LLVM IR.

Why this exists
---------------
Lifted GPU (AMDGPU/RDNA) code often contains NO `br` instructions: divergent
control flow is encoded as exec-mask updates (s_and_saveexec / s_andn2_saveexec /
s_or exec) plus `select`s, all inside one LLVM basic block. A literal CFG of such
IR is a single node. This tool therefore builds the CFG in two steps:

  1. Parse real LLVM blocks and terminators (br / switch / invoke / ...).
  2. Split every block at exec-mask writes to recover the *implicit* divergent
     control flow:
        and  exec, exec, mask   -> divergent IF   (head ends, THEN block starts)
        and  exec, saved, ~exec -> ELSE           (new block, edge from head if THEN skipped)
        or   exec, exec, saved  -> JOIN/reconverge

Nodes show what each block does (v_cmp_eq, v_mul_u32_u24, exec changes, memory ops),
not the raw IR text (use --full if you do want raw IR in the nodes).

Outputs (in --outdir, named after the input file):
  <stem>_blocks.csv        one row per block (role, ops, preds/succs, counts, ...)
  <stem>_edges.csv         one row per edge (src, dst, kind)
  <stem>_instructions.csv  one row per IR instruction with its (recovered) block
  <stem>_<function>.dot    Graphviz source
  <stem>_<function>.<fmt>  rendered graph (Graphviz `dot`, else matplotlib PNG)

Usage:
  python llvm_cfg.py lifted.ll -o out
  python llvm_cfg.py lifted.ll --format svg
  python llvm_cfg.py lifted.ll --no-exec-split     # literal LLVM CFG only
  python llvm_cfg.py lifted.ll --exec-ptr 'exec_lo' # name of the exec-mask pointer arg
"""
import argparse
import csv
import re
import shutil
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

NAME = r'"[^"]+"|[\w.$-]+'
TERMINATORS = {
    "ret", "br", "switch", "indirectbr", "invoke", "callbr",
    "resume", "unreachable", "cleanupret", "catchret", "catchswitch",
}
LABEL_LINE = re.compile(r'^(%s):\s*$' % NAME)
DEFINE_LINE = re.compile(r'^define\b.*?@(%s)\s*\(' % NAME)
RESULT_RE = re.compile(r'^(%(?:"[^"]+"|[^\s=]+))\s*=\s*(.*)$')
LABEL_REF = re.compile(r'label\s+%%(%s)' % NAME)
CALLEE_RE = re.compile(r'@(%s)\s*\(' % NAME)
VEC_RE = re.compile(r'<\d+ x ')
SSA_RE = re.compile(r'%(?:"[^"]+"|[\w.$-]+)')


# --------------------------------------------------------------------------- model
@dataclass
class Inst:
    index: int          # position inside the ORIGINAL LLVM block
    text: str
    result: str
    opcode: str
    callee: str = ""


@dataclass
class Block:
    name: str
    insts: List[Inst] = field(default_factory=list)
    succs: List[Tuple[str, str]] = field(default_factory=list)  # (dst, kind)
    preds: List[str] = field(default_factory=list)
    role: str = ""      # e.g. "entry", "THEN", "ELSE", "JOIN"
    orig: str = ""      # original LLVM block this piece came from
    ops: List[str] = field(default_factory=list)  # human-readable operation summary

    @property
    def terminator(self) -> Optional[Inst]:
        if self.insts and self.insts[-1].opcode in TERMINATORS:
            return self.insts[-1]
        return None


@dataclass
class Function:
    name: str
    blocks: Dict[str, Block] = field(default_factory=dict)

    @property
    def entry(self) -> Optional[str]:
        return next(iter(self.blocks), None)


# --------------------------------------------------------------------------- parsing
def strip_comment(line: str) -> str:
    in_q = False
    for i, ch in enumerate(line):
        if ch == '"':
            in_q = not in_q
        elif ch == ';' and not in_q:
            return line[:i]
    return line


def parse_inst(index: int, text: str) -> Inst:
    result, rest = "", text
    m = RESULT_RE.match(text)
    if m:
        result, rest = m.group(1), m.group(2)
    words = rest.split()
    i = 0
    while i < len(words) and words[i] in ("tail", "musttail", "notail"):
        i += 1
    op = words[i] if i < len(words) else ""
    callee = ""
    if op in ("call", "invoke", "callbr"):
        cm = CALLEE_RE.search(rest)
        callee = cm.group(1) if cm else ""
    return Inst(index, text, result, op, callee)


def edges_for(term: Inst) -> List[Tuple[str, str]]:
    text, op = term.text, term.opcode
    targets = LABEL_REF.findall(text)
    if op == "br":
        if len(targets) == 2:
            return [(targets[0], "true"), (targets[1], "false")]
        return [(t, "uncond") for t in targets]
    if op == "switch":
        head, _, body = text.partition("[")
        out = [(t, "default") for t in LABEL_REF.findall(head)]
        for val, dst in re.findall(r'\bi\d+\s+(-?\w+)\s*,\s*label\s+%%(%s)' % NAME, body):
            out.append((dst, "case %s" % val))
        return out
    if op == "invoke":
        kinds = ["normal", "unwind"]
        return [(t, kinds[i] if i < 2 else "?") for i, t in enumerate(targets)]
    if op == "callbr":
        return [(t, "fallthrough" if i == 0 else "indirect") for i, t in enumerate(targets)]
    if op == "indirectbr":
        return [(t, "indirect") for t in targets]
    return [(t, "") for t in targets]


def compute_preds(f: Function) -> None:
    for b in f.blocks.values():
        b.preds = []
    for b in f.blocks.values():
        for dst, _ in b.succs:
            if dst in f.blocks and b.name not in f.blocks[dst].preds:
                f.blocks[dst].preds.append(b.name)


def parse_module(source: str) -> List[Function]:
    functions: List[Function] = []
    fn: Optional[Function] = None
    cur: Optional[Block] = None
    pending = ""

    for raw in source.splitlines():
        line = strip_comment(raw).strip()
        if not line:
            continue
        if fn is None:
            m = DEFINE_LINE.match(line)
            if m:
                fn, cur, pending = Function(m.group(1).strip('"')), None, ""
            continue
        if line == "}":
            functions.append(fn)
            fn, cur = None, None
            continue
        if pending:
            pending += " " + line
            if "]" in line:
                line, pending = pending, ""
            else:
                continue
        elif re.search(r'\b(switch|indirectbr)\b', line) and "[" in line and "]" not in line:
            pending = line
            continue
        lm = LABEL_LINE.match(line)
        if lm:
            name = lm.group(1).strip('"')
            cur = fn.blocks.setdefault(name, Block(name, orig=name))
            continue
        if cur is None:
            cur = fn.blocks.setdefault("0", Block("0", orig="0"))
        cur.insts.append(parse_inst(len(cur.insts), line))

    for f in functions:
        for b in f.blocks.values():
            t = b.terminator
            if t:
                b.succs = [(d.strip('"'), k) for d, k in edges_for(t)]
        compute_preds(f)
    return functions


# --------------------------------------------------------------------------- exec-mask recovery
def is_vec(i: Inst) -> bool:
    return bool(VEC_RE.search(i.text))


def operands(i: Inst) -> List[str]:
    rest = i.text.split("=", 1)[1] if i.result else i.text
    return SSA_RE.findall(rest)


def slice_min(block: Block, defs: Dict[str, Inst], name: str) -> int:
    """Earliest instruction index in the operand slice that computes `name`."""
    seen, stack, best = set(), [name], len(block.insts)
    while stack:
        n = stack.pop()
        if n in seen or n not in defs:
            continue
        seen.add(n)
        d = defs[n]
        best = min(best, d.index)
        stack.extend(operands(d))
    return best


def find_exec_events(b: Block, exec_re: "re.Pattern") -> List[dict]:
    """Return exec-mask writes classified as if / else / join."""
    defs = {i.result: i for i in b.insts if i.result}
    events = []
    for i in b.insts:
        if i.opcode != "store":
            continue
        m = re.match(r'store\s+\S+\s+(%[^\s,]+),\s*ptr\s+%([^\s,]+)', i.text)
        if not m or not exec_re.fullmatch(m.group(2)):
            continue
        val = m.group(1)
        d = defs.get(val)
        if d is None:
            continue
        ops = operands(d)
        src = "mask"
        if d.opcode == "or":
            kind = "join"
        elif d.opcode == "and":
            inverted = any(o in defs and defs[o].opcode == "xor" and re.search(r',\s*-1\b', defs[o].text)
                           for o in ops)
            kind = "else" if inverted else "if"
            if kind == "if":
                for o in ops:
                    od = defs.get(o)
                    if od is not None and od.opcode == "load":
                        pm = re.search(r'ptr\s+%([^\s,]+)', od.text)
                        if pm and not exec_re.fullmatch(pm.group(1)):
                            src = pm.group(1)
        else:
            continue
        if kind == "if":   # head ends after trailing scalar bookkeeping, before next vector op
            cut = next((j.index for j in b.insts if j.index > i.index and is_vec(j)), None)
        else:              # else / join blocks begin where their exec-update group begins
            cut = slice_min(b, defs, val)
        events.append(dict(pos=i.index, kind=kind, cut=cut, src=src))
    return events


def const_of(text: str) -> str:
    m = re.search(r'splat \(i\d+ (-?\d+)\)', text)
    return m.group(1) if m else ""


def describe(i: Inst, defs: Dict[str, Inst], ev_at: Dict[int, dict]) -> Optional[str]:
    t, vec = i.text, is_vec(i)
    if i.index in ev_at:
        ev = ev_at[i.index]
        return {"if": "s_and_saveexec: s <- exec; exec &= %s" % ev["src"],
                "else": "s_andn2_saveexec: exec = s & ~exec  (else lanes)",
                "join": "s_or: exec |= s  (reconverge)"}[ev["kind"]]
    if i.opcode == "and" and vec and "16777215" not in t:
        c = const_of(t)
        return "v_and_b32 (& %s)" % c if c else "v_and_b32"
    if i.opcode == "icmp" and vec:
        return "v_cmp_%s" % t.split("icmp", 1)[1].split()[0]
    if i.opcode == "mul" and vec:
        return "v_mul_u32_u24"
    if i.opcode == "shl" and vec:
        c = const_of(t)
        return "v_lshlrev_b32 (<< %s)" % c if c else "v_lshlrev_b32"
    if i.opcode == "xor" and not vec and not re.search(r',\s*-1\b', t):
        return "s_xor_b32"
    if i.opcode == "call" and "masked.scatter" in i.callee:
        return "global_store (masked scatter, active lanes)"
    if i.opcode == "call" and "masked.gather" in i.callee:
        return "global_load (masked gather, active lanes)"
    if i.opcode == "load" and not vec:
        pm = re.search(r'ptr\s+(%[^\s,]+)', t)
        if pm and pm.group(1) in defs and defs[pm.group(1)].opcode == "inttoptr":
            return "s_load (scalar memory)"
    if i.opcode == "store" and not vec and "ptr %" in t and False:
        return None
    return None


def split_block(b: Block, is_entry: bool, exec_re) -> Tuple[List[Block], List[Tuple[str, str, str]]]:
    defs = {i.result: i for i in b.insts if i.result}
    events = find_exec_events(b, exec_re)
    ev_at = {e["pos"]: e for e in events}
    cuts = []
    for e in events:
        if e["cut"] is not None and 0 < e["cut"] < len(b.insts):
            cuts.append((e["cut"], {"if": "THEN", "else": "ELSE", "join": "JOIN"}[e["kind"]]))
    cuts.sort()
    # drop cuts that would create empty pieces
    clean, last = [], 0
    for idx, role in cuts:
        if idx > last:
            clean.append((idx, role))
            last = idx
    bounds = [0] + [c[0] for c in clean] + [len(b.insts)]
    roles = ["entry" if is_entry else "block"] + [c[1] for c in clean]

    pieces, counts = [], Counter()
    for k in range(len(roles)):
        insts = b.insts[bounds[k]:bounds[k + 1]]
        if k == 0:
            name = b.name
        else:
            counts[roles[k]] += 1
            n = counts[roles[k]]
            name = "%s.%s%s" % (b.name, roles[k].lower(), "" if n == 1 else n)
        p = Block(name, insts=insts, role=roles[k], orig=b.name)
        p.ops = [d for d in (describe(i, defs, ev_at) for i in insts) if d]
        pieces.append(p)

    edges: List[Tuple[str, str, str]] = []
    stack: List[dict] = []
    for k in range(1, len(pieces)):
        prev, cur, role = pieces[k - 1], pieces[k], roles[k]
        if role == "THEN":
            stack.append(dict(head=prev.name, els=None))
            edges.append((prev.name, cur.name, "exec != 0 (lanes take THEN)"))
        elif role == "ELSE":
            fr = stack[-1] if stack else None
            edges.append((prev.name, cur.name, "fallthrough"))
            if fr:
                fr["els"] = cur.name
                if fr["head"] != prev.name:
                    edges.append((fr["head"], cur.name, "execz (no lane took THEN)"))
        else:  # JOIN
            fr = stack.pop() if stack else None
            edges.append((prev.name, cur.name, "fallthrough"))
            if fr and fr["els"] is None and fr["head"] != prev.name:
                edges.append((fr["head"], cur.name, "execz (skip THEN)"))
    return pieces, edges


def apply_exec_split(f: Function, exec_re) -> bool:
    new_blocks: Dict[str, Block] = {}
    changed = False
    for b in f.blocks.values():
        pieces, internal = split_block(b, b.name == f.entry, exec_re)
        if len(pieces) > 1:
            changed = True
        pieces[-1].succs = list(b.succs)   # original terminator edges leave from the last piece
        by_name = {p.name: p for p in pieces}
        for s, d, kind in internal:
            by_name[s].succs.append((d, kind))
        if len(pieces) == 1:
            pieces[0].ops = pieces[0].ops
        for p in pieces:
            new_blocks[p.name] = p
    f.blocks = new_blocks
    compute_preds(f)
    return changed


# --------------------------------------------------------------------------- CSV
def write_csvs(functions: List[Function], prefix: Path) -> List[Path]:
    paths = []

    p = Path(str(prefix) + "_blocks.csv")
    with open(p, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["function", "block", "role", "orig_llvm_block", "n_ir_instructions",
                    "terminator", "successors", "predecessors", "operations", "n_loads",
                    "n_stores", "n_calls"])
        for f in functions:
            for b in f.blocks.values():
                ops = Counter(i.opcode for i in b.insts)
                w.writerow([f.name, b.name, b.role, b.orig, len(b.insts),
                            b.terminator.opcode if b.terminator else "",
                            ";".join(d for d, _ in b.succs), ";".join(b.preds),
                            " | ".join(b.ops), ops["load"], ops["store"],
                            ops["call"] + ops["invoke"]])
    paths.append(p)

    p = Path(str(prefix) + "_edges.csv")
    with open(p, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["function", "src", "dst", "kind"])
        for f in functions:
            for b in f.blocks.values():
                for dst, kind in b.succs:
                    w.writerow([f.name, b.name, dst, kind])
    paths.append(p)

    p = Path(str(prefix) + "_instructions.csv")
    with open(p, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["function", "block", "orig_llvm_block", "orig_index", "result",
                    "opcode", "callee", "text"])
        for f in functions:
            for b in f.blocks.values():
                for i in b.insts:
                    w.writerow([f.name, b.name, b.orig, i.index, i.result, i.opcode,
                                i.callee, i.text])
    paths.append(p)
    return paths


# --------------------------------------------------------------------------- labels
def block_label_lines(b: Block, full: bool, width: int) -> List[str]:
    title = b.name + ("   [%s]" % b.role if b.role else "")
    lines = [title, "-" * min(len(title), width)]
    if full:
        for i in b.insts:
            lines.append(i.text if len(i.text) <= width else i.text[: width - 3] + "...")
        return lines
    if b.ops:
        lines.extend(o if len(o) <= width else o[: width - 3] + "..." for o in b.ops)
    else:
        ops = Counter(i.opcode for i in b.insts)
        lines.extend("%s x%d" % kv for kv in ops.most_common(6))
    t = b.terminator
    if t:
        lines.append("terminator: %s" % t.opcode)
    lines.append("(%d IR instructions)" % len(b.insts))
    return lines


# --------------------------------------------------------------------------- DOT
def dot_escape(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


ROLE_FILL = {"entry": "#d9ead3", "THEN": "#fff2cc", "ELSE": "#fce5cd", "JOIN": "#d0e0f3"}


def edge_color(kind: str) -> str:
    if kind.startswith("exec !=") or kind == "true":
        return "darkgreen"
    if kind.startswith("execz") or kind == "false":
        return "firebrick"
    if kind == "unwind":
        return "orange"
    return "black"


def to_dot(f: Function, full: bool, width: int) -> str:
    out = ['digraph "%s" {' % dot_escape(f.name),
           '  rankdir=TB; nodesep=0.6; ranksep=0.6;',
           '  node [shape=box, style="rounded,filled", fontname="Courier", fontsize=10, fillcolor="#f4f6fa"];',
           '  edge [fontname="Helvetica", fontsize=9];',
           '  labelloc=t; label="Control-flow graph: @%s";' % dot_escape(f.name)]
    for b in f.blocks.values():
        label = "".join(dot_escape(l) + "\\l" for l in block_label_lines(b, full, width))
        fill = ROLE_FILL.get(b.role, "#f4f6fa")
        if b.terminator and b.terminator.opcode in ("ret", "unreachable", "resume"):
            fill = "#f4cccc"
        out.append('  "%s" [label="%s", fillcolor="%s"];' % (dot_escape(b.name), label, fill))
    for b in f.blocks.values():
        for dst, kind in b.succs:
            c = edge_color(kind)
            lab = "" if kind == "uncond" else kind
            out.append('  "%s" -> "%s" [label="%s", color="%s", fontcolor="%s"%s];' % (
                dot_escape(b.name), dot_escape(dst), dot_escape(lab), c, c,
                ", style=dashed" if kind.startswith("execz") else ""))
    out.append("}")
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------- rendering
def render_with_dot(dot_path: Path, fmt: str) -> Optional[Path]:
    exe = shutil.which("dot")
    if not exe:
        return None
    out = dot_path.with_suffix("." + fmt)
    subprocess.run([exe, "-T" + fmt, str(dot_path), "-o", str(out)], check=True)
    return out


def render_with_matplotlib(f: Function, out: Path, full: bool, width: int) -> Optional[Path]:
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        from matplotlib.patches import FancyArrowPatch
    except ImportError:
        return None

    layer: Dict[str, int] = {}
    order = [f.entry] if f.entry else []
    if f.entry:
        layer[f.entry] = 0
    for name in order:
        for dst, _ in f.blocks[name].succs:
            if dst in f.blocks and dst not in layer:
                layer[dst] = layer[name] + 1
                order.append(dst)
    # longest-path layering so fallthrough chains stack vertically
    for _ in range(len(f.blocks)):
        for b in f.blocks.values():
            for dst, _k in b.succs:
                if dst in layer and b.name in layer and layer[dst] <= layer[b.name] and dst != b.name \
                        and layer[dst] < len(f.blocks):
                    layer[dst] = layer[b.name] + 1
    for name in f.blocks:
        layer.setdefault(name, max(layer.values(), default=0) + 1)

    rows: Dict[int, List[str]] = {}
    for n, l in layer.items():
        rows.setdefault(l, []).append(n)
    texts = {n: "\n".join(block_label_lines(b, full, width)) for n, b in f.blocks.items()}
    max_lines = max(t.count("\n") + 1 for t in texts.values())
    max_cols = max(max(len(x) for x in t.split("\n")) for t in texts.values())
    node_w, node_h = max_cols * 0.085 + 0.5, max_lines * 0.17 + 0.4
    gap_x, gap_y = 0.8, 0.9

    pos = {}
    for l, names in rows.items():
        total = len(names) * node_w + (len(names) - 1) * gap_x
        for i, n in enumerate(names):
            pos[n] = (-total / 2 + i * (node_w + gap_x) + node_w / 2, -l * (node_h + gap_y))

    fig, ax = plt.subplots(figsize=(max(6, max(len(v) for v in rows.values()) * (node_w + gap_x)),
                                    max(4, len(rows) * (node_h + gap_y))))
    ax.set_axis_off()
    ax.set_title("Control-flow graph: @%s" % f.name)
    for n, (x, y) in pos.items():
        ax.text(x, y, texts[n], ha="center", va="center", family="monospace", fontsize=7,
                bbox=dict(boxstyle="round,pad=0.4", fc=ROLE_FILL.get(f.blocks[n].role, "#f4f6fa"), ec="black"))
    for b in f.blocks.values():
        for dst, kind in b.succs:
            if dst not in pos:
                continue
            (x1, y1), (x2, y2) = pos[b.name], pos[dst]
            skip = abs(layer[dst] - layer[b.name]) > 1
            ax.add_patch(FancyArrowPatch((x1, y1 - node_h / 2), (x2, y2 + node_h / 2), arrowstyle="->",
                                         mutation_scale=12, color=edge_color(kind),
                                         linestyle="--" if kind.startswith("execz") else "-",
                                         connectionstyle="arc3,rad=%s" % (0.35 if skip else 0.0)))
            if kind and kind != "uncond":
                ax.text((x1 + x2) / 2 + (node_w * 0.45 if skip else 0), (y1 + y2) / 2, kind,
                        fontsize=7, color=edge_color(kind))
    xs, ys = [p[0] for p in pos.values()], [p[1] for p in pos.values()]
    ax.set_xlim(min(xs) - node_w, max(xs) + node_w * 1.5)
    ax.set_ylim(min(ys) - node_h, max(ys) + node_h)
    fig.savefig(out, dpi=150, bbox_inches="tight")
    plt.close(fig)
    return out


# --------------------------------------------------------------------------- main
def main() -> int:
    ap = argparse.ArgumentParser(description="LLVM IR -> control-flow graph (diagram + CSV)")
    ap.add_argument("input", help="path to .ll file")
    ap.add_argument("-o", "--outdir", default=".", help="output directory (default: .)")
    ap.add_argument("--format", default="png", choices=["png", "svg", "pdf"], help="diagram format")
    ap.add_argument("--no-exec-split", action="store_true",
                    help="do not recover divergent control flow from exec-mask writes")
    ap.add_argument("--exec-ptr", default=r"exec(_lo)?",
                    help="regex for the exec-mask pointer argument name (default: exec(_lo)?)")
    ap.add_argument("--full", action="store_true", help="put raw IR text in nodes instead of summaries")
    ap.add_argument("--width", type=int, default=90, help="max chars per label line")
    ap.add_argument("--no-render", action="store_true", help="write CSV + .dot only")
    args = ap.parse_args()

    src, outdir = Path(args.input), Path(args.outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    prefix = outdir / src.stem

    functions = parse_module(src.read_text())
    if not functions:
        print("No function definitions found in %s" % src, file=sys.stderr)
        return 1

    if not args.no_exec_split:
        exec_re = re.compile(args.exec_ptr)
        for f in functions:
            if not apply_exec_split(f, exec_re):
                print("note: @%s has no exec-mask control flow to recover" % f.name)

    for p in write_csvs(functions, prefix):
        print("wrote", p)

    for f in functions:
        safe = re.sub(r'[^\w.-]', "_", f.name)
        dot_path = Path("%s_%s.dot" % (prefix, safe))
        dot_path.write_text(to_dot(f, args.full, args.width))
        print("wrote", dot_path)
        if not args.no_render:
            img = render_with_dot(dot_path, args.format)
            if img is None:
                img = render_with_matplotlib(f, Path("%s_%s.png" % (prefix, safe)), args.full, args.width)
            if img:
                print("wrote", img)
            else:
                print("Could not render: install Graphviz (`dot`) or matplotlib.", file=sys.stderr)
        print("@%s: %d block(s), %d edge(s)" % (f.name, len(f.blocks),
                                                 sum(len(b.succs) for b in f.blocks.values())))
    return 0


if __name__ == "__main__":
    sys.exit(main())

