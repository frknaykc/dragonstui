#!/usr/bin/env python3
"""Bounded local M69 runner; measures the real Rust host and its fixture children."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
TEST = "multi_adapter_stream_pressure_preserves_rpc_and_queue_accounting"


def cpu_seconds(value):
    """POSIX ps time: [days-]hours:minutes:seconds or minutes:seconds."""
    days, separator, clock = value.partition("-")
    total = int(days) * 86400 if separator else 0
    parts = (clock if separator else value).split(":")
    if len(parts) not in (2, 3):
        raise ValueError(f"unsupported ps CPU time: {value}")
    result = 0.0
    for part in parts:
        result = result * 60 + float(part)
    return total + result


def parse_ps(text):
    result = {}
    for line in text.splitlines():
        pid, rss, cpu, state = line.split()
        # macOS ps resets a zombie's CPU and RSS to zero before it is reaped.
        if state.startswith("Z"):
            continue
        result[int(pid)] = {"rss_bytes": int(rss) * 1024, "cpu_seconds": cpu_seconds(cpu)}
    return result


def sample(pids):
    completed = subprocess.run(
        ["ps", "-o", "pid=,rss=,time=,stat=", "-p", ",".join(map(str, pids))],
        capture_output=True, text=True, timeout=3, check=False,
    )
    if completed.returncode not in (0, 1):
        raise RuntimeError(f"ps failed: {completed.stderr}")
    return parse_ps(completed.stdout)


def group_alive(pid):
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False


def cleanup_group(process):
    """Only the fresh session owned by this run, never a discovered user process."""
    for sig in (signal.SIGTERM, signal.SIGKILL):
        if not group_alive(process.pid):
            break
        os.killpg(process.pid, sig)
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass
    process.wait(timeout=3)


def summarize_samples(samples, pids):
    complete = [(stamp, rows) for stamp, rows in samples if set(pids) <= rows.keys()]
    if len(complete) < 2:
        raise RuntimeError("fewer than two complete process samples; increase --rounds")
    first_time, first = complete[0]
    last_time, last = complete[-1]
    span = last_time - first_time
    groups = {"host": pids[:1], "adapters": pids[1:]}
    result = {"complete_samples": len(complete), "sample_span_seconds": span}
    for name, members in groups.items():
        cpu = sum(last[pid]["cpu_seconds"] - first[pid]["cpu_seconds"] for pid in members)
        if cpu < 0:
            raise RuntimeError("non-monotonic CPU counters")
        result[name] = {
            "cpu_seconds_delta": cpu,
            "cpu_percent_one_core": 100 * cpu / span,
            "sampled_peak_rss_bytes": max(
                sum(rows[pid]["rss_bytes"] for pid in members) for _, rows in complete
            ),
            "first_rss_bytes": sum(first[pid]["rss_bytes"] for pid in members),
            "last_rss_bytes": sum(last[pid]["rss_bytes"] for pid in members),
        }
    return result


def run_case(binary, adapters, rounds, timeout):
    with tempfile.TemporaryDirectory(prefix="dragonstui-m69-run-") as directory:
        log_path = Path(directory) / "test.log"
        environment = dict(os.environ, TMPDIR=directory,
                           DRAGONSTUI_STRESS_ADAPTERS=str(adapters),
                           DRAGONSTUI_STRESS_ROUNDS=str(rounds))
        with log_path.open("w+") as log:
            process = subprocess.Popen(
                [str(binary), TEST, "--exact", "--nocapture"], cwd=ROOT,
                env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
            )
            pids = None
            samples = []
            deadline = time.monotonic() + timeout
            try:
                while process.poll() is None:
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"stress run exceeded {timeout}s")
                    if pids is None:
                        for line in log_path.read_text().splitlines():
                            if line.startswith("M69_READY "):
                                ready = json.loads(line.split(" ", 1)[1])
                                pids = [ready["host_pid"], *ready["adapter_pids"]]
                                if pids[0] != process.pid or len(set(pids)) != adapters + 1:
                                    raise RuntimeError("unexpected fixture process identities")
                    if pids:
                        samples.append((time.monotonic(), sample(pids)))
                    time.sleep(0.1)
                text = log_path.read_text()
                if process.returncode != 0:
                    raise RuntimeError(f"Rust stress test failed ({process.returncode}):\n{text}")
                results = [json.loads(line.split(" ", 1)[1]) for line in text.splitlines()
                           if line.startswith("M69_RESULT ")]
                cleanups = [json.loads(line.split(" ", 1)[1]) for line in text.splitlines()
                            if line.startswith("M69_CLEANUP ")]
                if len(results) != 2 or {r["held_consumer"] for r in results} != {False, True}:
                    raise RuntimeError("missing fast/held consumer evidence")
                if cleanups != [{"stopped_adapters": adapters}] or group_alive(process.pid):
                    raise RuntimeError("fixture process cleanup not proven")
                for result in results:
                    if result["adapters"] != adapters or result["rounds"] != rounds:
                        raise RuntimeError("requested workload was not run")
                    if result["delivered_events"] + result["dropped_events"] != result["emitted_events"]:
                        raise RuntimeError("event accounting mismatch")
                return {"scenarios": results, "resources": summarize_samples(samples, pids),
                        "cleanup": "all fixture processes exited"}
            except Exception as error:
                # Include bounded fixture output before the private temporary root is removed.
                raise RuntimeError(
                    f"{error}\nResource samples: {json.dumps(samples[-20:])}"
                    f"\nFixture log:\n{log_path.read_text()[-16000:]}"
                ) from error
            finally:
                cleanup_group(process)


def build():
    command = ["cargo", "test", "-p", "dragonstui-adapter-host", "--release",
               "--test", "multi_adapter_stress", "--no-run", "--message-format=json"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise RuntimeError(result.stderr + result.stdout)
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binaries = [item["executable"] for item in artifacts
                if item.get("reason") == "compiler-artifact" and item.get("executable")
                and item["target"]["name"] == "multi_adapter_stress"]
    if len(binaries) != 1:
        raise RuntimeError("expected exactly one compiled stress test binary")
    return Path(binaries[0])


def bounded_int(low, high):
    def parse(value):
        number = int(value)
        if not low <= number <= high:
            raise argparse.ArgumentTypeError(f"expected {low}..{high}")
        return number
    return parse


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapters", nargs="+", type=bounded_int(1, 16), default=[1, 4, 8])
    parser.add_argument("--rounds", type=bounded_int(1, 20000), default=5000)
    parser.add_argument("--timeout", type=bounded_int(1, 600), default=120)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("resource sampling requires POSIX ps and process groups")
    binary = build()
    report = {
        "schema_version": 1, "platform": platform.platform(), "profile": "release",
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_sha256": {
            name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
            for name in [
                "tools/adapter_stress.py",
                "crates/dragonstui-adapter-host/tests/multi_adapter_stress.rs",
                "crates/dragonstui-adapter-host/src/bin/dragonstui_adapter_host_mock.rs",
                "crates/dragonstui-adapter-host/src/manager.rs",
                "crates/dragonstui-adapter-host/src/runtime.rs",
                "crates/dragonstui-adapter-host/src/process.rs",
            ]
        },
        "measurement": "100ms ps samples; CPU delta over complete sample window; RSS is sampled, not allocator peak",
        "cases": [], "status": "in_progress",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        for count in args.adapters:
            report["cases"].append(run_case(binary, count, args.rounds, args.timeout))
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "cases": len(report["cases"]),
                      "report": str(args.output)}))


if __name__ == "__main__":
    main()
