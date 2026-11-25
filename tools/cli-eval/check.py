#!/usr/bin/env python3
"""What an agent left in evaluation libraries after tasks.md:
python3 tools/cli-eval/check.py <library>…  (QUIET_BIN: the quiet to read them with)"""
import json, os, subprocess, sys
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
Q = os.environ.get("QUIET_BIN", os.path.join(ROOT, "target", "release", "quiet"))

def q(lib, *args):
    r = subprocess.run([Q, "--json", *args], capture_output=True, text=True, env={**os.environ, "QUIET_MD_HOME": lib})
    return json.loads(r.stdout) if r.returncode == 0 and r.stdout.strip() else None

def check(lib):
    out = {}
    # 2: the new risk is in 方案's 风险 list, after the two old risks.
    b = q(lib, "read", "项目/方案#风险", "--section", "--as", "json") or {"blocks": []}
    texts = [x["text"] for x in b["blocks"]]
    out["2 risk added in section"] = any("第三方库许可证需要审核" in t for t in texts) and texts[-1:] != [] and "许可证" in "".join(texts)
    # 3: 待办汇总 at the top, one heading per meeting, the six open todos.
    t = q(lib, "blocks", "待办汇总")
    if t:
        heads = [x["text"] for x in t["blocks"] if x["kind"].startswith("h")]
        todos = [x["text"] for x in t["blocks"] if x["kind"] in ("todo", "bullet")]
        want = ["调研 PDF 分页方案", "给搜索加按标题优先排序", "PDF 分页方案评审", "移动端阅读体验走查", "准备灰度名单", "更新帮助文档里的导出章节"]
        out["3 todos collected"] = all(any(w in x for x in todos) for w in want) and not any("整理导出问题清单" in x for x in todos)
        out["3 grouped by meeting"] = sum(1 for h in heads if any(k in h for k in ("11-03", "11-10", "11-17"))) >= 3
    else:
        out["3 todos collected"] = out["3 grouped by meeting"] = False
    # 4: 评分 5 → 推荐, 置身事内 added.
    tb = q(lib, "table", "书单") or {"rows": []}
    rows = tb["rows"]
    out["4 five-star rows 推荐"] = all(r.get("状态") == "推荐" for r in rows if r.get("评分") == "5") and any(r.get("评分") == "5" for r in rows)
    out["4 others untouched"] = all(r.get("状态") != "推荐" for r in rows if r.get("评分") not in ("5",))
    out["4 book added"] = any(r.get("书名") == "置身事内" and r.get("作者") == "兰小欢" and r.get("状态") == "想读" for r in rows)
    # 5: board 项目流程: 5 labelled boxes left to right, 4 bound arrows in order.
    bd = q(lib, "board", "项目流程")
    if bd:
        els = bd["elements"]
        boxes = [e for e in els if e["type"] in ("rectangle", "ellipse", "diamond") and e.get("label")]
        order = [e["label"] for e in sorted(boxes, key=lambda e: e["x"])]
        out["5 boxes in order"] = order == ["需求调研", "方案设计", "开发", "测试", "发布"]
        ids = {e["label"]: e["id"] for e in boxes}
        arrows = {(e.get("from"), e.get("to")) for e in els if e["type"] == "arrow"}
        steps = ["需求调研", "方案设计", "开发", "测试", "发布"]
        out["5 arrows bound step to step"] = all((ids.get(a), ids.get(b)) in arrows for a, b in zip(steps, steps[1:]))
    else:
        out["5 boxes in order"] = out["5 arrows bound step to step"] = False
    # 6: 周报 still has its 风险 section with both items.
    w = q(lib, "read", "周报#风险", "--section", "--as", "json")
    out["6 风险 restored"] = bool(w) and any("PDF 分页方案还没定" in x["text"] for x in w["blocks"]) and any("测试人力不足" in x["text"] for x in w["blocks"])
    # 7: four documents in 方案拆分 without a repeated heading; 方案 lists them (mentions count extra).
    ls = q(lib, "ls", "方案拆分") or []
    titles = sorted(e["title"] for e in ls)
    out["7 four documents"] = titles == sorted(["背景", "目标", "设计", "风险"])
    first = [((q(lib, "blocks", e["path"]) or {"blocks": [{}]})["blocks"] or [{}])[0] for e in ls]
    out["7 no repeated heading"] = bool(first) and not any(f.get("kind", "").startswith("h") and f.get("text") in titles for f in first)
    links = q(lib, "links", "项目/方案") or {"out": []}
    out["7 linked as mentions (bonus)"] = sum(1 for l in links["out"] if (l.get("to") or "").startswith("方案拆分/")) >= 4
    md = (q(lib, "read", "项目/方案", "--max-chars", "0") or {"text": ""})["text"]
    out["7 list appended"] = all(t in md.split("## 风险")[-1] or f"[[方案拆分/{t}]]" in md for t in ["背景", "目标", "设计", "风险"])
    return out

for lib in sys.argv[1:]:
    r = check(os.path.abspath(lib))
    print(f"\n{lib}: {sum(r.values())}/{len(r)}")
    for k, v in r.items(): print(f"  {'✓' if v else '✗'} {k}")
