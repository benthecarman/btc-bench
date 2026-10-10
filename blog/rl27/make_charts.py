"""Charts for the rl27 blog post. Data comes from runs/rl27 (training
batches, held-out evals) and the bench-s42-lite runs; numbers that come
from logs are stated inline. Run: uv run --no-project --with matplotlib python blog/rl27/make_charts.py"""
import glob, json, re, statistics as st
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch, Rectangle

OUT = "blog/rl27"
SURFACE, INK, INK2, MUTED, GRID = "#fcfcfb", "#0b0b0b", "#52514e", "#8a8984", "#e6e5e0"
BLUE, ORANGE, BASE = "#2a78d6", "#eb6834", "#b4b2ab"
plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 11, "text.color": INK, "axes.labelcolor": INK2,
    "axes.edgecolor": GRID, "axes.facecolor": SURFACE, "figure.facecolor": SURFACE,
    "xtick.color": INK2, "ytick.color": INK2, "axes.spines.top": False, "axes.spines.right": False,
    "axes.grid": True, "grid.color": GRID, "grid.linewidth": 0.8, "axes.axisbelow": True,
})

def save(fig, name):
    for ext in ("png", "svg"):
        fig.savefig(f"{OUT}/{name}.{ext}", dpi=200, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)

def title(ax, t, sub):
    ax.set_title(t, loc="left", fontsize=14, fontweight="bold", pad=26)
    ax.text(0, 1.02, sub, transform=ax.transAxes, fontsize=10.5, color=INK2, va="bottom")

def hbar(ax, y, w, h, color):
    ax.barh(y, w, height=h, color=color, linewidth=0)

# Per-kind means on bench-s42-lite submit, read from each run's graded
# results (re-grade with `btc-bench grade` after grader changes).
# Unanswered tasks count as 0, as in the grade report.
KINDS = ["write", "optimize", "identify", "tree", "judgment", "overall"]
PREFIX = {"write": "t1", "optimize": "t2", "identify": "t3", "tree": "t4", "judgment": "t5", "overall": "t"}
TASK_IDS = [json.loads(l)["id"] for l in open("datasets/bench-s42-lite/fixtures.jsonl")]

def suite_means(run):
    scores = {r["task_id"]: r["score"] for r in json.load(open(f"runs/{run}/bench-s42-lite-submit/graded/results.json"))}
    out = []
    for k in KINDS:
        ids = [i for i in TASK_IDS if i.startswith(PREFIX[k])]
        out.append(sum(scores.get(i, 0.0) for i in ids) / len(ids))
    return out

BASE_MEANS = suite_means("baseline-27b")
RL_MEANS = suite_means("rl27-bench")
OPUS_MEANS = suite_means("opus55-bench")

# 1. Headline: base vs step 28 per task kind on bench-s42-lite submit.
def headline():
    kinds = KINDS
    base, rl = BASE_MEANS, RL_MEANS
    rel = [f"+{(r / b - 1) * 100:.0f}%" for b, r in zip(base, rl)]
    fig, ax = plt.subplots(figsize=(8, 5.2))
    ax.grid(axis="y", visible=False)
    h = 0.32
    for i, k in enumerate(kinds):
        y = len(kinds) - 1 - i
        hbar(ax, y + 0.19, base[i], h, BASE)
        hbar(ax, y - 0.19, rl[i], h, BLUE)
        ax.text(base[i] + 0.008, y + 0.19, f"{base[i]:.3f}", va="center", fontsize=9.5, color=INK2)
        ax.text(rl[i] + 0.008, y - 0.19, f"{rl[i]:.3f}  ({rel[i]})", va="center", fontsize=9.5, color=INK,
                fontweight="bold" if k == "overall" else "normal")
    ax.set_yticks(range(len(kinds)))
    ax.set_yticklabels(list(reversed(kinds)))
    ax.get_yticklabels()[0].set_fontweight("bold")
    ax.axhline(0.5, color=GRID, linewidth=1)
    ax.set_xlim(0, 1.0)
    ax.set_ylim(-0.6, len(kinds) - 0.4)
    ax.set_xlabel("mean score (0 to 1)")
    ax.tick_params(axis="y", length=0)
    title(ax, f"RL lifted the 27B model's score by {(rl[-1] / base[-1] - 1) * 100:.0f}%",
          "bench-s42-lite, 246 tasks, one attempt each, same checkpoint and sampling")
    ax.legend(handles=[Rectangle((0, 0), 1, 1, color=BASE), Rectangle((0, 0), 1, 1, color=BLUE)],
              labels=["Qwen3.8-27B (base)", "after RL (step-28 LoRA)"], loc="lower right", frameon=False)
    save(fig, "1-headline-by-kind")

# 2. Held-out score and answer length per checkpoint (two panels, one x axis).
def trajectory():
    steps = [0, 8, 18, 28, 38, 40]
    score = [0.314, 0.393, 0.399, 0.491, 0.429, 0.434]
    length = [17.0, 6.4, 21.1, 24.1, 43.5, 48.5]
    fig, (a1, a2) = plt.subplots(2, 1, figsize=(8, 6.4), sharex=True, gridspec_kw={"height_ratios": [1.15, 1]})
    for ax in (a1, a2):
        ax.grid(axis="x", visible=False)
        ax.axvspan(-1, 24.5, color="#f0efec", zorder=0, linewidth=0)
    a1.plot(steps, score, color=BLUE, linewidth=2, marker="o", markersize=7,
            markeredgecolor=SURFACE, markeredgewidth=2, solid_capstyle="round")
    a1.annotate("best: step 28, 0.491", (28, 0.491), xytext=(30.5, 0.50), fontsize=10, color=INK,
                arrowprops=dict(arrowstyle="-", color=MUTED, linewidth=1))
    a1.text(1.2, 0.302, "base 0.314", fontsize=9.5, color=INK2)
    a1.set_ylabel("held-out score")
    a1.set_ylim(0.28, 0.53)
    a2.plot(steps, length, color=ORANGE, linewidth=2, marker="o", markersize=7,
            markeredgecolor=SURFACE, markeredgewidth=2)
    a2.text(40.6, 48.5, "48.5k", va="center", fontsize=9.5, color=INK2)
    a2.text(28.6, 22.0, "24.1k", va="center", fontsize=9.5, color=INK2)
    a2.set_ylabel("median answer (k tokens)")
    a2.set_ylim(0, 56)
    a2.set_xlabel("RL step (checkpoint)")
    a2.set_xlim(-1, 42)
    a1.text(12, 0.515, "32k rollout budget (batches 1–24)", fontsize=9.5, color=INK2, ha="center")
    title(a1, "Score peaked at step 28, then answers kept growing",
          "80 held-out tasks, mean of 2 samples per checkpoint (step 8: 1 sample)")
    fig.align_ylabels((a1, a2))
    save(fig, "2-checkpoint-trajectory")

# 3. Training rollout length, correct vs incorrect (pooled 5-batch windows).
def training_length():
    batches = {}
    for f in glob.glob("runs/rl27/batches/batch-*.jsonl"):
        b = int(re.findall(r"\d+", f)[-1])
        batches[b] = [(sum(r["response_mask"]), r["reward"] > 0) for r in map(json.loads, open(f))]
    xs, ok, bad = [], [], []
    for b in range(5, 41):
        pool = [x for k in range(b - 4, b + 1) for x in batches[k]]
        xs.append(b)
        ok.append(st.median(l for l, c in pool if c) / 1000)
        bad.append(st.median(l for l, c in pool if not c) / 1000)
    fig, ax = plt.subplots(figsize=(8, 4.6))
    ax.grid(axis="x", visible=False)
    ax.axvspan(0, 24.5, color="#f0efec", zorder=0, linewidth=0)
    ax.axhline(32.768, color=MUTED, linewidth=1)
    ax.text(5.3, 33.6, "32,768-token budget", fontsize=9.5, color=INK2)
    ax.text(25.2, 2.0, "budget removed →", fontsize=9.5, color=INK2)
    ax.plot(xs, bad, color=ORANGE, linewidth=2, solid_capstyle="round")
    ax.plot(xs, ok, color=BLUE, linewidth=2, solid_capstyle="round")
    ax.text(40.5, bad[-1], "failed", va="center", fontsize=10, color=INK)
    ax.text(40.5, ok[-1], "solved", va="center", fontsize=10, color=INK)
    ax.set_xlim(4.5, 43)
    ax.set_ylim(0, 48)
    ax.set_xlabel("training batch")
    ax.set_ylabel("median rollout length (k tokens)")
    title(ax, "Without the budget, all answers got longer",
          "training rollouts, median over a sliding 5-batch window")
    ax.legend([plt.Line2D([], [], color=ORANGE, linewidth=2), plt.Line2D([], [], color=BLUE, linewidth=2)],
              ["failed attempts (reward 0)", "solved attempts (reward > 0)"], loc="upper left", frameon=False)
    save(fig, "3-training-length")

# 4. Spark: pausing on heat vs locking the GPU clock (13-minute windows each).
def spark_clock():
    labels = ["full clock,\npause at 85°C", "locked\n2000 MHz", "locked\n1700 MHz", "locked\n1400 MHz"]
    work = [0.30, 0.69, 0.70, 0.58]
    notes = ["8 pauses, peak 89°C", "2 pauses, peak 85°C", "0 pauses, peak 79°C", "0 pauses, peak 74°C"]
    fig, ax = plt.subplots(figsize=(8, 4.6))
    ax.grid(axis="x", visible=False)
    for i, w in enumerate(work):
        ax.bar(i, w, width=0.34, color=BLUE if i == 2 else BASE, linewidth=0)
        ax.text(i, w + 0.055, f"{w:.2f}", ha="center", fontsize=10.5, color=INK, fontweight="bold" if i == 2 else "normal")
        ax.text(i, w + 0.02, notes[i], ha="center", fontsize=8.8, color=INK2)
    ax.set_xticks(range(4))
    ax.set_xticklabels(labels)
    ax.tick_params(axis="x", length=0, pad=8)
    ax.set_xlim(-0.6, 3.6)
    ax.set_ylim(0, 0.85)
    ax.set_ylabel("training throughput\n(fraction of full speed)")
    title(ax, "Slower clocks trained 2.3× faster than pausing",
          "DGX Spark during training, 13 min per setting; estimated as time training × clock speed")
    save(fig, "4-spark-clock-lock")

# 5. Setup diagram.
def setup():
    fig, ax = plt.subplots(figsize=(9, 3.9))
    ax.axis("off"); ax.set_xlim(0, 10); ax.set_ylim(0, 4.4)
    def box(x, y, w, h, head, lines):
        ax.add_patch(FancyBboxPatch((x, y), w, h, boxstyle="round,pad=0,rounding_size=0.18",
                                    facecolor="#f3f2ef", edgecolor="#d9d8d3", linewidth=1))
        ax.text(x + 0.25, y + h - 0.38, head, fontsize=12.5, fontweight="bold", va="top")
        for i, l in enumerate(lines):
            ax.text(x + 0.25, y + h - 0.95 - i * 0.42, l, fontsize=10, color=INK2, va="top")
    box(0.2, 0.6, 4.0, 3.0, "RTX 5090 · generates", ["Qwen3.8-27B, 4-bit NVFP4 (vLLM)", "+ current LoRA adapter",
        "8 tasks × 8 attempts per step", "graded by the benchmark's oracle"])
    box(5.8, 0.6, 4.0, 3.0, "DGX Spark · trains", ["same model in bf16", "rank-32 LoRA, GRPO-style update",
        "one optimizer step per batch", "GPU locked at 1700 MHz"])
    ax.annotate("", xy=(5.75, 2.75), xytext=(4.25, 2.75),
                arrowprops=dict(arrowstyle="-|>", color=BLUE, linewidth=2, mutation_scale=16))
    ax.text(5.0, 2.95, "scored attempts", ha="center", fontsize=10, color=INK)
    ax.annotate("", xy=(4.25, 1.45), xytext=(5.75, 1.45),
                arrowprops=dict(arrowstyle="-|>", color=ORANGE, linewidth=2, mutation_scale=16))
    ax.text(5.0, 1.0, "new adapter", ha="center", fontsize=10, color=INK)
    ax.text(0.2, 4.15, "Two machines, one loop: no rented GPUs", fontsize=14, fontweight="bold")
    save(fig, "5-setup-diagram")

# 6. Headline with Claude Opus 5.5 for reference (same 246 tasks, same runner).
def headline_vs_opus():
    kinds = KINDS
    base, rl, opus = BASE_MEANS, RL_MEANS, OPUS_MEANS
    closed = (rl[-1] - base[-1]) / (opus[-1] - base[-1])
    fig, ax = plt.subplots(figsize=(8, 6.4))
    ax.grid(axis="y", visible=False)
    h, off = 0.24, 0.27
    for i, k in enumerate(kinds):
        y = len(kinds) - 1 - i
        bold = "bold" if k == "overall" else "normal"
        for dy, vals, col, ink in ((off, base, BASE, INK2), (0, rl, BLUE, INK), (-off, opus, ORANGE, INK2)):
            hbar(ax, y + dy, vals[i], h, col)
            ax.text(vals[i] + 0.01, y + dy, f"{vals[i]:.3f}", va="center", fontsize=9, color=ink,
                    fontweight=bold if col == BLUE else "normal")
    ax.set_yticks(range(len(kinds)))
    ax.set_yticklabels(list(reversed(kinds)))
    ax.get_yticklabels()[0].set_fontweight("bold")
    ax.axhline(0.5, color=GRID, linewidth=1)
    ax.set_xlim(0, 1.12)
    ax.set_xticks([0, 0.2, 0.4, 0.6, 0.8, 1.0])
    ax.set_ylim(-0.6, len(kinds) - 0.4)
    ax.set_xlabel("mean score (0 to 1)")
    ax.tick_params(axis="y", length=0)
    title(ax, f"RL closed {closed:.0%} of the gap to Opus 5.5",
          "bench-s42-lite, 246 tasks, one attempt each; Opus 5.5 at default (medium) effort")
    ax.legend(handles=[Rectangle((0, 0), 1, 1, color=c) for c in (BASE, BLUE, ORANGE)],
              labels=["Qwen3.8-27B (base)", "after RL (step-28 LoRA)", "Claude Opus 5.5"],
              loc="upper center", bbox_to_anchor=(0.45, -0.1), ncol=3, frameon=False)
    save(fig, "6-headline-vs-opus")

headline(); trajectory(); training_length(); spark_clock(); setup(); headline_vs_opus()
print("wrote", sorted(glob.glob(f"{OUT}/*.png")))
