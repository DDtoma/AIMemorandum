"""Check this proposal's links, AC references, OpenAPI structure and JS syntax.
This is a document checker, not an application or full OpenAPI conformance test.
"""
import ast
import json
import re
import subprocess
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent
errors = []
markdown = sorted(ROOT.rglob("*.md"))
links = 0
for path in markdown:
    content = path.read_text(encoding="utf-8")
    for raw in re.findall(r"\]\(([^)]+)\)", content):
        if raw.startswith(("http:", "https:", "#", "mailto:")):
            continue
        target = raw.split("#", 1)[0]
        links += 1
        if not (path.parent / target).resolve().exists():
            errors.append(f"Missing link in {path.relative_to(ROOT)}: {target}")
    if content.count("```") % 2:
        errors.append(f"Unclosed code fence: {path.relative_to(ROOT)}")

acceptance_path = ROOT / "00_shared/03_performance_and_acceptance.md"
definitions = re.findall(r"^\| (AC-[A-Z]+-\d{3}) \|", acceptance_path.read_text(), re.M)
if len(set(definitions)) != len(definitions):
    errors.append("Duplicate acceptance criteria")
known = set(definitions)
for path in markdown:
    for ac in re.findall(r"AC-[A-Z]+-\d{3}", path.read_text()):
        if ac not in known:
            errors.append(f"Undefined AC {ac}: {path.relative_to(ROOT)}")

tasks = re.findall(r"^- \[ \] (T\d{3})：(.+)$", (ROOT / "00_shared/05_task_checklist.md").read_text(), re.M)
task_ids = [tid for tid, _ in tasks]
if len(set(task_ids)) != len(task_ids):
    errors.append("Duplicate task IDs")
for tid, description in tasks:
    if "AC-" not in description or "`" not in description:
        errors.append(f"Task missing AC or path: {tid}")

spec = json.loads((ROOT / "contracts/openapi.json").read_text())
if spec.get("openapi") != "3.1.0":
    errors.append("Unexpected OpenAPI version")
references = 0


def walk(value, location="#"):
    global references
    if isinstance(value, dict):
        if "$ref" in value:
            reference = value["$ref"]
            references += 1
            cursor = spec
            try:
                assert reference.startswith("#/")
                for part in reference[2:].split("/"):
                    cursor = cursor[part.replace("~1", "/").replace("~0", "~")]
            except (AssertionError, KeyError):
                errors.append(f"Broken schema reference: {reference}")
        if value.get("type") == "object" and "properties" in value:
            for field in value.get("required", []):
                if field not in value["properties"]:
                    errors.append(f"Required property missing at {location}: {field}")
        for k, v in value.items():
            walk(v, f"{location}/{k}")
    elif isinstance(value, list):
        for i, v in enumerate(value):
            walk(v, f"{location}/{i}")


walk(spec)
operations = []
for route, methods in spec["paths"].items():
    for method, op in methods.items():
        operations.append(op["operationId"])
        if "{id}" in route and not any(p["name"] == "id" and p["in"] == "path" and p["required"] for p in op.get("parameters", [])):
            errors.append(f"Missing path parameter: {method} {route}")
        if not any(key.startswith("2") for key in op["responses"]):
            errors.append(f"No success response: {route}")
        if method in {"post", "put", "patch", "delete"} and op.get("security") != []:
            if not any(p["name"] == "X-Sync-Epoch" and p["required"] for p in op["parameters"]):
                errors.append(f"Write without epoch: {route}")
if len(set(operations)) != len(operations):
    errors.append("Duplicate operationId")


class Markup(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids, self.scripts = [], []
        self.in_script = False
    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if "id" in values:
            self.ids.append(values["id"])
        if tag == "script":
            self.in_script = True
    def handle_endtag(self, tag):
        if tag == "script":
            self.in_script = False
    def handle_data(self, data):
        if self.in_script:
            self.scripts.append(data)


markup = Markup()
markup.feed((ROOT / "01_product/03_wireframes.html").read_text())
if len(set(markup.ids)) != len(markup.ids):
    errors.append("Duplicate HTML IDs")
js = "\n".join(markup.scripts)
for match in re.findall(r"byId\('([^']+)'\)", js):
    if match not in markup.ids:
        errors.append(f"Missing HTML target {match}")
try:
    checked = subprocess.run(["node", "--check"], input=js, text=True, capture_output=True)
    if checked.returncode:
        errors.append(checked.stderr)
except FileNotFoundError:
    errors.append("node unavailable; JS syntax not checked")
for path in ROOT.rglob("*.py"):
    ast.parse(path.read_text(), filename=str(path))

result = {"markdown_files": len(markdown), "relative_links": links,
    "acceptance_criteria": len(known), "development_tasks": len(tasks),
    "openapi_paths": len(spec["paths"]), "openapi_operations": len(operations),
    "openapi_schemas": len(spec["components"]["schemas"]), "schema_references": references,
    "html_ids": len(markup.ids), "errors": errors,
    "scope": "Document structure only; no product implementation, native Mac testing, or formal OpenAPI validator."}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(1 if errors else 0)
