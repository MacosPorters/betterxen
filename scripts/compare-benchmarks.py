#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bounded comparison tool for structured Xenora benchmark result sets.

Compares two benchmark run directories (each containing metadata.json and
results.json) only when their revision-independent configuration matches.
Reports medians, dispersion (IQR and min-max span), missing or excluded entries,
and factual uncertainty notes without applying an arbitrary pass/fail threshold.
"""
import argparse
import json
import os
import sys
import tempfile
import unittest

REVISION_DEPENDENT_KEYS = frozenset({"run_id", "timestamp", "commit"})

REQUIRED_CONFIG_KEYS = (
    "device",
    "host_arch",
    "os_version",
    "os_build",
    "rustc",
    "xcode",
    "profile",
    "api_validation",
    "shader_validation",
    "power",
    "thermal",
    "reps",
    "warmup",
)

METRICS = ("compile", "library", "pipeline", "gpu", "host")
STAT_KEYS = ("median", "min", "max", "q1", "q3", "reps")


def _load_json(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def load_run(run_dir):
    meta_path = os.path.join(run_dir, "metadata.json")
    results_path = os.path.join(run_dir, "results.json")
    if not os.path.isfile(meta_path):
        raise FileNotFoundError("missing metadata.json in %s" % run_dir)
    if not os.path.isfile(results_path):
        raise FileNotFoundError("missing results.json in %s" % run_dir)
    return _load_json(meta_path), _load_json(results_path)


def check_config_compatibility(base_meta, cand_meta):
    """Return a list of revision-independent configuration mismatches."""
    mismatches = []
    all_keys = sorted(
        (set(REQUIRED_CONFIG_KEYS) | set(base_meta.keys()) | set(cand_meta.keys()))
        - REVISION_DEPENDENT_KEYS
    )
    for key in all_keys:
        in_base = key in base_meta and base_meta[key] not in (None, "")
        in_cand = key in cand_meta and cand_meta[key] not in (None, "")
        if key in REQUIRED_CONFIG_KEYS and (not in_base or not in_cand):
            mismatches.append(
                {
                    "field": key,
                    "baseline": base_meta.get(key),
                    "candidate": cand_meta.get(key),
                    "reason": "missing_required_field",
                }
            )
            continue
        if base_meta.get(key) != cand_meta.get(key):
            mismatches.append(
                {
                    "field": key,
                    "baseline": base_meta.get(key),
                    "candidate": cand_meta.get(key),
                    "reason": "value_mismatch",
                }
            )
    return mismatches


def _validate_stat_block(block):
    if not isinstance(block, dict):
        return False
    for k in STAT_KEYS:
        v = block.get(k)
        if not isinstance(v, (int, float)) or isinstance(v, bool):
            return False
    return True


def _summarize_block(block):
    median = float(block["median"])
    q1 = float(block["q1"])
    q3 = float(block["q3"])
    lo = float(block["min"])
    hi = float(block["max"])
    reps = int(block["reps"])
    iqr = q3 - q1
    return {
        "median": median,
        "q1": q1,
        "q3": q3,
        "iqr": iqr,
        "min": lo,
        "max": hi,
        "span": hi - lo,
        "reps": reps,
        "relative_iqr": (iqr / median) if median > 0.0 else None,
    }


def _compare_metric(metric_name, base_size, cand_size):
    base_block = base_size.get(metric_name)
    cand_block = cand_size.get(metric_name)
    if not _validate_stat_block(base_block) or not _validate_stat_block(cand_block):
        return None, {
            "metric": metric_name,
            "baseline_present": _validate_stat_block(base_block),
            "candidate_present": _validate_stat_block(cand_block),
        }

    b = _summarize_block(base_block)
    c = _summarize_block(cand_block)
    delta = c["median"] - b["median"]
    ratio = (c["median"] / b["median"]) if b["median"] > 0.0 else None
    pct = ((delta / b["median"]) * 100.0) if b["median"] > 0.0 else None
    iqr_overlap = max(b["q1"], c["q1"]) <= min(b["q3"], c["q3"])
    combined_iqr = b["iqr"] + c["iqr"]
    delta_to_iqr = (abs(delta) / combined_iqr) if combined_iqr > 0.0 else None

    notes = []
    if iqr_overlap:
        notes.append("iqr_overlap")
    if b["reps"] < 5 or c["reps"] < 5:
        notes.append("low_sample_count")
    if b["median"] == 0.0:
        notes.append("zero_baseline_median")

    entry = {
        "metric": metric_name,
        "baseline": b,
        "candidate": c,
        "median_delta": delta,
        "median_ratio": ratio,
        "median_pct_change": pct,
        "iqr_overlap": iqr_overlap,
        "combined_iqr": combined_iqr,
        "delta_to_combined_iqr": delta_to_iqr,
        "uncertainty_notes": notes,
    }

    if metric_name == "gpu":
        bz = base_size.get("gpu_zero_fraction")
        cz = cand_size.get("gpu_zero_fraction")
        entry["gpu_zero_fraction_baseline"] = bz
        entry["gpu_zero_fraction_candidate"] = cz
        if (isinstance(bz, (int, float)) and bz > 0.0) or (
            isinstance(cz, (int, float)) and cz > 0.0
        ):
            notes.append("gpu_timer_zero_samples")

    return entry, None


def compare_runs(base_meta, base_results, cand_meta, cand_results):
    """Compare two structured benchmark runs. Never applies an arbitrary pass threshold."""
    mismatches = check_config_compatibility(base_meta, cand_meta)
    report = {
        "compatible": len(mismatches) == 0,
        "baseline": {
            "run_id": base_meta.get("run_id"),
            "commit": base_meta.get("commit"),
            "timestamp": base_meta.get("timestamp"),
        },
        "candidate": {
            "run_id": cand_meta.get("run_id"),
            "commit": cand_meta.get("commit"),
            "timestamp": cand_meta.get("timestamp"),
        },
        "config_mismatches": mismatches,
        "missing_fixtures": {"baseline_only": [], "candidate_only": []},
        "excluded_fixtures": [],
        "comparisons": [],
    }
    if mismatches:
        return report

    base_fixtures = {
        (f.get("name"), f.get("stage"), f.get("source")): f
        for f in (base_results.get("fixtures") or [])
    }
    cand_fixtures = {
        (f.get("name"), f.get("stage"), f.get("source")): f
        for f in (cand_results.get("fixtures") or [])
    }

    all_keys = sorted(set(base_fixtures.keys()) | set(cand_fixtures.keys()))
    for key in all_keys:
        name, stage, source = key
        bf = base_fixtures.get(key)
        cf = cand_fixtures.get(key)
        if bf is None:
            report["missing_fixtures"]["candidate_only"].append(
                {"name": name, "stage": stage, "source": source}
            )
            continue
        if cf is None:
            report["missing_fixtures"]["baseline_only"].append(
                {"name": name, "stage": stage, "source": source}
            )
            continue

        if bf.get("status") != "benchmarked" or cf.get("status") != "benchmarked":
            report["excluded_fixtures"].append(
                {
                    "name": name,
                    "stage": stage,
                    "source": source,
                    "baseline_status": bf.get("status"),
                    "baseline_reason": bf.get("reason", ""),
                    "candidate_status": cf.get("status"),
                    "candidate_reason": cf.get("reason", ""),
                }
            )
            continue

        base_sizes = {s.get("words"): s for s in (bf.get("sizes") or []) if "words" in s}
        cand_sizes = {s.get("words"): s for s in (cf.get("sizes") or []) if "words" in s}
        all_words = sorted(set(base_sizes.keys()) | set(cand_sizes.keys()))

        fixture_comp = {
            "name": name,
            "stage": stage,
            "source": source,
            "missing_sizes": {"baseline_only": [], "candidate_only": []},
            "sizes": [],
        }

        for w in all_words:
            bs = base_sizes.get(w)
            cs = cand_sizes.get(w)
            if bs is None:
                fixture_comp["missing_sizes"]["candidate_only"].append(w)
                continue
            if cs is None:
                fixture_comp["missing_sizes"]["baseline_only"].append(w)
                continue

            metrics_out = []
            missing_metrics = []
            for m in METRICS:
                entry, miss = _compare_metric(m, bs, cs)
                if miss is not None:
                    missing_metrics.append(miss)
                else:
                    metrics_out.append(entry)

            fixture_comp["sizes"].append(
                {
                    "words": w,
                    "metrics": metrics_out,
                    "missing_metrics": missing_metrics,
                }
            )

        report["comparisons"].append(fixture_comp)

    return report


def _fmt_us(seconds):
    if seconds is None:
        return "-"
    return "%.3f us" % (seconds * 1e6)


def format_text(report):
    lines = [
        "Xenora benchmark comparison (no arbitrary pass/fail threshold)",
        "Baseline:  %s (commit %s)"
        % (report["baseline"]["run_id"], report["baseline"]["commit"]),
        "Candidate: %s (commit %s)"
        % (report["candidate"]["run_id"], report["candidate"]["commit"]),
    ]
    if not report["compatible"]:
        lines.append("")
        lines.append("Status: INCOMPATIBLE (revision-independent configuration mismatch)")
        for m in report["config_mismatches"]:
            lines.append(
                "- %s (%s): baseline=%r candidate=%r"
                % (m["field"], m["reason"], m["baseline"], m["candidate"])
            )
        return "\n".join(lines) + "\n"

    lines.append("Status: COMPATIBLE")

    mf = report["missing_fixtures"]
    if mf["baseline_only"] or mf["candidate_only"]:
        lines.append("")
        lines.append("Missing fixtures:")
        for item in mf["baseline_only"]:
            lines.append("- baseline_only: %s (%s/%s)" % (item["name"], item["stage"], item["source"]))
        for item in mf["candidate_only"]:
            lines.append("- candidate_only: %s (%s/%s)" % (item["name"], item["stage"], item["source"]))

    if report["excluded_fixtures"]:
        lines.append("")
        lines.append("Excluded fixtures (not timed):")
        for ex in report["excluded_fixtures"]:
            lines.append(
                "- %s: baseline=%s (%s) candidate=%s (%s)"
                % (
                    ex["name"],
                    ex["baseline_status"],
                    ex["baseline_reason"],
                    ex["candidate_status"],
                    ex["candidate_reason"],
                )
            )

    for fc in report["comparisons"]:
        lines.append("")
        lines.append("Fixture: %s (%s, %s)" % (fc["name"], fc["stage"], fc["source"]))
        ms = fc["missing_sizes"]
        if ms["baseline_only"] or ms["candidate_only"]:
            lines.append(
                "  Missing sizes: baseline_only=%s candidate_only=%s"
                % (ms["baseline_only"], ms["candidate_only"])
            )
        lines.append(
            "  words\tmetric\tbase_med\tbase_iqr\tcand_med\tcand_iqr\tdelta_pct\tuncertainty"
        )
        for s in fc["sizes"]:
            for m in s["metrics"]:
                pct_str = (
                    "%+.2f%%" % m["median_pct_change"]
                    if m["median_pct_change"] is not None
                    else "n/a"
                )
                notes = ",".join(m["uncertainty_notes"]) or "none"
                lines.append(
                    "  %d\t%s\t%s\t%s\t%s\t%s\t%s\t%s"
                    % (
                        s["words"],
                        m["metric"],
                        _fmt_us(m["baseline"]["median"]),
                        _fmt_us(m["baseline"]["iqr"]),
                        _fmt_us(m["candidate"]["median"]),
                        _fmt_us(m["candidate"]["iqr"]),
                        pct_str,
                        notes,
                    )
                )
            for mm in s["missing_metrics"]:
                lines.append(
                    "  %d\t%s\tMISSING (baseline=%s candidate=%s)"
                    % (s["words"], mm["metric"], mm["baseline_present"], mm["candidate_present"])
                )

    return "\n".join(lines) + "\n"


def _sample_meta(**overrides):
    base = {
        "run_id": "run-1",
        "timestamp": "unix:100",
        "device": "Apple M5",
        "host_arch": "aarch64",
        "os_version": "27.3",
        "os_build": "26C5047e",
        "rustc": "rustc 1.100.0",
        "xcode": "Xcode 27.2",
        "commit": "aaaa1111",
        "profile": "release",
        "api_validation": "0",
        "shader_validation": "0",
        "power": "AC Power",
        "thermal": "no warnings recorded",
        "reps": 21,
        "warmup": 2,
    }
    base.update(overrides)
    return base


def _sample_stat(median, q1, q3, lo=None, hi=None, reps=21):
    return {
        "median": median,
        "q1": q1,
        "q3": q3,
        "min": q1 if lo is None else lo,
        "max": q3 if hi is None else hi,
        "reps": reps,
    }


def _sample_size(words, med=10.0e-6, q1=9.0e-6, q3=11.0e-6, reps=21, gpu_zero=0.0):
    return {
        "words": words,
        "compile": _sample_stat(med, q1, q3, reps=reps),
        "library": _sample_stat(med * 2, q1 * 2, q3 * 2, reps=reps),
        "pipeline": _sample_stat(med * 3, q1 * 3, q3 * 3, reps=reps),
        "gpu": _sample_stat(med * 0.5, q1 * 0.5, q3 * 0.5, reps=reps),
        "host": _sample_stat(med * 10, q1 * 10, q3 * 10, reps=reps),
        "gpu_zero_fraction": gpu_zero,
    }


class CompareBenchmarksSelfTest(unittest.TestCase):
    def test_rejects_revision_independent_config_mismatch(self):
        bm = _sample_meta(run_id="r1", commit="111", power="Battery Power", reps=7)
        cm = _sample_meta(run_id="r2", commit="222", power="AC Power", reps=21)
        rep = compare_runs(bm, {"fixtures": []}, cm, {"fixtures": []})
        self.assertFalse(rep["compatible"])
        fields = {m["field"] for m in rep["config_mismatches"]}
        self.assertEqual(fields, {"power", "reps"})

    def test_rejects_missing_required_metadata(self):
        bm = _sample_meta()
        cm = _sample_meta()
        del cm["api_validation"]
        rep = compare_runs(bm, {"fixtures": []}, cm, {"fixtures": []})
        self.assertFalse(rep["compatible"])
        self.assertEqual(rep["config_mismatches"][0]["field"], "api_validation")
        self.assertEqual(rep["config_mismatches"][0]["reason"], "missing_required_field")

    def test_allows_revision_dependent_differences_and_computes_exact_stats(self):
        bm = _sample_meta(run_id="r1", commit="111", timestamp="unix:100")
        cm = _sample_meta(run_id="r2", commit="222", timestamp="unix:200")
        br = {
            "fixtures": [
                {
                    "name": "add",
                    "stage": "compute",
                    "source": "translated-rdna",
                    "status": "benchmarked",
                    "reason": "",
                    "sizes": [_sample_size(64, med=10.0e-6, q1=9.0e-6, q3=11.0e-6)],
                }
            ]
        }
        cr = {
            "fixtures": [
                {
                    "name": "add",
                    "stage": "compute",
                    "source": "translated-rdna",
                    "status": "benchmarked",
                    "reason": "",
                    "sizes": [_sample_size(64, med=15.0e-6, q1=14.0e-6, q3=16.0e-6)],
                }
            ]
        }
        rep = compare_runs(bm, br, cm, cr)
        self.assertTrue(rep["compatible"])
        comp = rep["comparisons"][0]["sizes"][0]["metrics"][0]
        self.assertEqual(comp["metric"], "compile")
        self.assertAlmostEqual(comp["baseline"]["iqr"], 2.0e-6, places=12)
        self.assertAlmostEqual(comp["candidate"]["iqr"], 2.0e-6, places=12)
        self.assertAlmostEqual(comp["median_delta"], 5.0e-6, places=12)
        self.assertAlmostEqual(comp["median_ratio"], 1.5, places=9)
        self.assertAlmostEqual(comp["median_pct_change"], 50.0, places=9)
        self.assertFalse(comp["iqr_overlap"])
        self.assertAlmostEqual(comp["delta_to_combined_iqr"], 1.25, places=9)
        self.assertEqual(comp["uncertainty_notes"], [])

    def test_reports_iqr_overlap_gpu_zero_and_low_samples(self):
        bm = _sample_meta(reps=3)
        cm = _sample_meta(reps=3)
        br = {
            "fixtures": [
                {
                    "name": "add",
                    "stage": "compute",
                    "source": "translated-rdna",
                    "status": "benchmarked",
                    "reason": "",
                    "sizes": [_sample_size(4, med=10.0e-6, q1=8.0e-6, q3=12.0e-6, reps=3, gpu_zero=0.25)],
                }
            ]
        }
        cr = {
            "fixtures": [
                {
                    "name": "add",
                    "stage": "compute",
                    "source": "translated-rdna",
                    "status": "benchmarked",
                    "reason": "",
                    "sizes": [_sample_size(4, med=11.0e-6, q1=9.0e-6, q3=13.0e-6, reps=3, gpu_zero=0.0)],
                }
            ]
        }
        rep = compare_runs(bm, br, cm, cr)
        gpu_m = [m for m in rep["comparisons"][0]["sizes"][0]["metrics"] if m["metric"] == "gpu"][0]
        self.assertTrue(gpu_m["iqr_overlap"])
        self.assertIn("iqr_overlap", gpu_m["uncertainty_notes"])
        self.assertIn("low_sample_count", gpu_m["uncertainty_notes"])
        self.assertIn("gpu_timer_zero_samples", gpu_m["uncertainty_notes"])

    def test_reports_missing_fixtures_sizes_metrics_and_excluded(self):
        bm = _sample_meta()
        cm = _sample_meta()
        size_missing_gpu = _sample_size(64)
        del size_missing_gpu["gpu"]
        br = {
            "fixtures": [
                {"name": "only-base", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4)]},
                {"name": "excl", "stage": "compute", "source": "translated-rdna", "status": "excluded", "reason": "oracle mismatch", "sizes": []},
                {"name": "shared", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4), size_missing_gpu]},
            ]
        }
        cr = {
            "fixtures": [
                {"name": "only-cand", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4)]},
                {"name": "excl", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4)]},
                {"name": "shared", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(64), _sample_size(256)]},
            ]
        }
        rep = compare_runs(bm, br, cm, cr)
        self.assertTrue(rep["compatible"])
        self.assertEqual([f["name"] for f in rep["missing_fixtures"]["baseline_only"]], ["only-base"])
        self.assertEqual([f["name"] for f in rep["missing_fixtures"]["candidate_only"]], ["only-cand"])
        self.assertEqual(len(rep["excluded_fixtures"]), 1)
        shared = rep["comparisons"][0]
        self.assertEqual(shared["missing_sizes"]["baseline_only"], [4])
        self.assertEqual(shared["missing_sizes"]["candidate_only"], [256])
        self.assertEqual(shared["sizes"][0]["missing_metrics"][0]["metric"], "gpu")

    def test_no_arbitrary_pass_threshold_on_large_slowdown(self):
        with tempfile.TemporaryDirectory() as td:
            d1 = os.path.join(td, "r1")
            d2 = os.path.join(td, "r2")
            os.makedirs(d1)
            os.makedirs(d2)
            with open(os.path.join(d1, "metadata.json"), "w", encoding="utf-8") as fh:
                json.dump(_sample_meta(run_id="r1", commit="aaa"), fh)
            with open(os.path.join(d2, "metadata.json"), "w", encoding="utf-8") as fh:
                json.dump(_sample_meta(run_id="r2", commit="bbb"), fh)
            with open(os.path.join(d1, "results.json"), "w", encoding="utf-8") as fh:
                json.dump({"fixtures": [{"name": "add", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4, med=10e-6, q1=9e-6, q3=11e-6)]}]}, fh)
            with open(os.path.join(d2, "results.json"), "w", encoding="utf-8") as fh:
                json.dump({"fixtures": [{"name": "add", "stage": "compute", "source": "translated-rdna", "status": "benchmarked", "reason": "", "sizes": [_sample_size(4, med=100e-6, q1=95e-6, q3=105e-6)]}]}, fh)
            # Must still exit 0 (factual reporting, no arbitrary pass/fail threshold).
            self.assertEqual(main([d1, d2, "--format", "json"]), 0)

    def test_real_repo_runs_flag_power_and_reps_mismatch(self):
        root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
        r1 = os.path.join(root, "benchmarks", "results", "apple-m5", "run-1795338391")
        r2 = os.path.join(root, "benchmarks", "results", "apple-m5", "run-1795352394")
        if os.path.isdir(r1) and os.path.isdir(r2):
            bm, br = load_run(r1)
            cm, cr = load_run(r2)
            rep = compare_runs(bm, br, cm, cr)
            self.assertFalse(rep["compatible"])
            self.assertEqual({m["field"] for m in rep["config_mismatches"]}, {"power", "reps"})


def main(argv=None):
    p = argparse.ArgumentParser(
        description="Compare two structured Xenora benchmark run directories."
    )
    p.add_argument("baseline", nargs="?", help="Baseline run directory")
    p.add_argument("candidate", nargs="?", help="Candidate run directory")
    p.add_argument("--format", choices=("text", "json"), default="text")
    p.add_argument("--self-test", action="store_true", help="Run built-in unit tests")
    args = p.parse_args(argv)

    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(CompareBenchmarksSelfTest)
        res = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if res.wasSuccessful() else 1

    if not args.baseline or not args.candidate:
        p.error("baseline and candidate run directories are required (or pass --self-test)")

    try:
        bm, br = load_run(args.baseline)
        cm, cr = load_run(args.candidate)
    except (OSError, json.JSONDecodeError) as exc:
        sys.stderr.write("error: %s\n" % exc)
        return 2

    report = compare_runs(bm, br, cm, cr)
    if args.format == "json":
        json.dump(report, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")
    else:
        sys.stdout.write(format_text(report))

    return 0 if report["compatible"] else 2


if __name__ == "__main__":
    sys.exit(main())
