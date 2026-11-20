#!/usr/bin/env python3
"""Build a pulled FableFlight checkout behind a lightweight update window."""

import argparse
import json
import os
from pathlib import Path
import queue
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parent.parent
INSTALL = ROOT / "target" / "launcher"
EXE = "fablegame.exe" if os.name == "nt" else "fablegame"
REVISION = re.compile(r"[0-9a-f]{40,64}")
ART = [
    ("Earth from orbit", "ui/launcher-orbit.png"),
    ("Gulfstream G650", "aircraft/g650.png"),
    ("Cessna 172RG", "aircraft/c172.png"),
    ("Boeing 747", "aircraft/747.png"),
]


def revision():
    result = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if not REVISION.fullmatch(result):
        raise ValueError("Could not identify the checkout's Git revision")
    return result


def previous_game():
    pointer = INSTALL / "current"
    if pointer.is_file():
        tag = pointer.read_text().strip()
        if REVISION.fullmatch(tag):
            path = INSTALL / tag / EXE
            if path.is_file():
                return path
    legacy = ROOT / "target" / "release" / EXE
    fallback = INSTALL / "previous" / EXE
    if not fallback.is_file() and legacy.is_file():
        copy_executable(legacy, fallback)
    return fallback if fallback.is_file() else None


def copy_executable(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        shutil.copy2(source, temporary)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def build(tag, notify, cancelled):
    destination = INSTALL / tag / EXE
    if destination.is_file():
        return destination
    if sys.platform.startswith("linux"):
        check = subprocess.run(
            ["bash", str(ROOT / "scripts" / "build-linux.sh"), "--check"],
            cwd=ROOT, capture_output=True, text=True,
        )
        if check.returncode:
            raise RuntimeError(check.stderr.strip() or "Linux build prerequisites check failed")
    if cancelled.is_set():
        return None
    INSTALL.mkdir(parents=True, exist_ok=True)
    log_path = INSTALL / "build.log"
    settings = {"start_new_session": True} if os.name != "nt" else {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
    notify(("build", "Building the simulator"))
    with log_path.open("w", encoding="utf-8") as log, subprocess.Popen(
        ["cargo", "build", "--locked", "--release", "--bin", "fablegame", "--message-format=json-render-diagnostics"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=log, text=True, errors="replace", **settings,
    ) as cargo:
        output = queue.Queue()

        def reader():
            for line in cargo.stdout:
                output.put(line)
            output.put(None)

        threading.Thread(target=reader, daemon=True).start()
        executable = None
        try:
            while not cancelled.is_set():
                try:
                    line = output.get(timeout=0.2)
                except queue.Empty:
                    continue
                if line is None:
                    break
                try:
                    event = json.loads(line)
                except ValueError:
                    log.write(line)
                    continue
                if event.get("reason") == "compiler-message":
                    log.write(event.get("message", {}).get("rendered") or "")
                if event.get("reason") == "compiler-artifact":
                    target = event.get("target", {}).get("name")
                    if not event.get("fresh") and target:
                        notify(("build", f"Built {target}"))
                    if target == "fablegame" and event.get("executable"):
                        executable = Path(event["executable"])
            if cancelled.is_set():
                return None
            if cargo.wait() != 0 or executable is None or not executable.is_file():
                raise RuntimeError(f"Build failed. Details are in {log_path}")
        finally:
            if cargo.poll() is None:
                try:
                    if os.name == "nt":
                        cargo.send_signal(signal.CTRL_BREAK_EVENT)
                    else:
                        os.killpg(cargo.pid, signal.SIGTERM)
                    cargo.wait(timeout=3)
                except (OSError, subprocess.TimeoutExpired):
                    cargo.kill()
    notify(("install", "Installing the update"))
    copy_executable(executable, destination)
    return destination


def launch(executable, args):
    subprocess.Popen([str(executable), *args], cwd=ROOT)


def update(notify, cancelled):
    tag = revision()
    ready = build(tag, notify, cancelled)
    if ready is None or cancelled.is_set():
        return
    with tempfile.NamedTemporaryFile(dir=INSTALL, delete=False, mode="w", encoding="utf-8") as pointer:
        pointer.write(tag + "\n")
        temporary = Path(pointer.name)
    try:
        temporary.replace(INSTALL / "current")
    finally:
        temporary.unlink(missing_ok=True)
    return ready if not cancelled.is_set() else None


def show_launcher(args, fallback):
    import tkinter as tk
    from tkinter import ttk

    root = tk.Tk()
    root.title("Fable Flight")
    root.geometry("660x510")
    root.minsize(660, 510)
    root.configure(bg="#10171b")
    style = ttk.Style(root)
    style.theme_use("clam")
    style.configure("Dispatch.Horizontal.TProgressbar", background="#96c3b6", troughcolor="#2a383c",
                    bordercolor="#2a383c", lightcolor="#96c3b6", darkcolor="#96c3b6", thickness=6)
    style.configure("Dispatch.TButton", background="#273238", foreground="#edf3f0",
                    bordercolor="#39474b", lightcolor="#273238", darkcolor="#273238", padding=(12, 6))
    style.map("Dispatch.TButton", background=[("active", "#39474b")],
              foreground=[("disabled", "#748589")])
    events = queue.Queue()
    stop = threading.Event()
    image = tk.Label(root, bg="#10171b", borderwidth=0)
    image.pack(fill="both", expand=True)
    body = tk.Frame(root, bg="#10171b", padx=28, pady=18)
    body.pack(fill="x")
    status = tk.StringVar(value="Checking this checkout")
    caption = tk.StringVar()
    tk.Label(body, textvariable=caption, anchor="w", bg="#10171b", fg="#9eafb5").pack(fill="x")
    tk.Label(body, textvariable=status, anchor="w", bg="#10171b", fg="#edf3f0",
             font=("TkDefaultFont", 17, "bold"), wraplength=600, justify="left").pack(fill="x", pady=(4, 12))
    progress = ttk.Progressbar(body, style="Dispatch.Horizontal.TProgressbar", mode="determinate", maximum=100, value=6)
    progress.pack(fill="x")
    buttons = tk.Frame(body, bg="#10171b")
    buttons.pack(fill="x", pady=(12, 0))

    def close():
        stop.set()
        root.withdraw()
        if worker.is_alive():
            root.after(100, close)
        else:
            root.destroy()

    def skip():
        if fallback is None:
            return
        stop.set()
        try:
            launch(fallback, args)
        except OSError as exc:
            status.set(str(exc))
            skip_button.configure(state="disabled")
        else:
            close()

    skip_button = ttk.Button(buttons, style="Dispatch.TButton", text="Skip update", command=skip, state="normal" if fallback else "disabled")
    skip_button.pack(side="right")
    ttk.Button(buttons, style="Dispatch.TButton", text="Close", command=close).pack(side="left")
    root.protocol("WM_DELETE_WINDOW", close)

    def show_art(index):
        name, filename = ART[index]
        picture = tk.PhotoImage(file=str(ROOT / "assets" / filename))
        image.configure(image=picture)
        image.photo = picture
        caption.set(name)
        root.after(5600, show_art, (index + 1) % len(ART))

    def work():
        try:
            executable = update(events.put, stop)
            if executable is not None:
                events.put(("done", executable))
        except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
            events.put(("error", str(exc)))

    worker = threading.Thread(target=work)

    def poll():
        while not events.empty():
            stage, message = events.get_nowait()
            if stage == "done":
                if stop.is_set():
                    return
                try:
                    launch(message, args)
                except OSError as exc:
                    stage, message = "error", str(exc)
                else:
                    skip_button.configure(state="disabled")
                    message = "Opening Fable Flight"
            status.set(message)
            if stage == "build" and progress["mode"] != "indeterminate":
                progress.configure(mode="indeterminate")
                progress.start(30)
            elif stage in ("install", "done", "error"):
                progress.stop()
                progress.configure(mode="determinate", value={"install": 95, "done": 100, "error": 0}[stage])
                if stage == "done":
                    root.after(300, root.destroy)
                    return
        root.after(100, poll)

    show_art(0)
    root.after(250, lambda: worker.start() if not stop.is_set() else None)
    root.after(100, poll)
    root.mainloop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-ui", action="store_true")
    parser.add_argument("--skip-update", action="store_true")
    parser.add_argument("game_args", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    args = options.game_args[1:] if options.game_args[:1] == ["--"] else options.game_args
    fallback = previous_game()
    if options.skip_update:
        if fallback is None:
            parser.error("No previous build is installed in this checkout")
        launch(fallback, args)
    elif options.no_ui:
        binary = update(lambda stage: print(stage[1], flush=True), threading.Event())
        if binary is not None:
            launch(binary, args)
    else:
        try:
            show_launcher(args, fallback)
        except ImportError:
            print("Python Tk is unavailable. Run with --no-ui.", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(f"Fable Flight: {error}")
