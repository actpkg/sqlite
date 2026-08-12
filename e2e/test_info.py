import json
import re
import subprocess


def test_manifest_name_starts_with_sqlite(act_command, wasm_path):
    out = subprocess.run(
        [*act_command, "inspect", "component-manifest", str(wasm_path)],
        capture_output=True, text=True, check=True,
    ).stdout
    manifest = json.loads(out)
    # hurl's `matches` is an unanchored search (verified against hurl 8.0.1),
    # not a full match — re.search, not re.fullmatch, so this keeps passing
    # for the "sqlite-vec" variant's manifest name too, exactly as the
    # original hurl assertion did for both variants.
    assert re.search(r"^sqlite", manifest["std"]["name"])
