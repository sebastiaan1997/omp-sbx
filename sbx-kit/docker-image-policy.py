#!/usr/bin/env python3
"""Compile a project Docker image allowlist into runtime AuthZ/Buildx policy."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
import tempfile
from pathlib import Path

import yaml

_NAME_COMPONENT = re.compile(r"^[a-z0-9]+(?:[._-][a-z0-9]+)*$")
_REGISTRY_COMPONENT = re.compile(r"^[a-z0-9]+(?:[.-][a-z0-9]+)*(?::[0-9]+)?$")
_TAG = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$")
_DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")


class PolicyError(ValueError):
    pass


def canonical_image(reference: str) -> str:
    if not reference or reference != reference.strip() or any(c.isspace() for c in reference):
        raise PolicyError(f"invalid image reference: {reference!r}")
    if "://" in reference or reference.count("@") > 1:
        raise PolicyError(f"invalid image reference: {reference!r}")

    name_and_tag, separator, digest = reference.partition("@")
    if separator and not _DIGEST.fullmatch(digest):
        raise PolicyError(f"image digest must be sha256:<64 lowercase hex>: {reference}")

    slash = name_and_tag.rfind("/")
    colon = name_and_tag.rfind(":")
    if colon > slash:
        name = name_and_tag[:colon]
        tag = name_and_tag[colon + 1 :]
        if not _TAG.fullmatch(tag):
            raise PolicyError(f"invalid image tag: {reference}")
    else:
        name = name_and_tag
        tag = ""

    if not name or name.lower() != name:
        raise PolicyError(f"image repository must be lowercase: {reference}")
    parts = name.split("/")
    if any(not part for part in parts):
        raise PolicyError(f"invalid image repository: {reference}")

    first = parts[0]
    if "." in first or ":" in first or first == "localhost":
        if not _REGISTRY_COMPONENT.fullmatch(first):
            raise PolicyError(f"invalid image registry: {reference}")
        registry = first
        repository = parts[1:]
        if not repository:
            raise PolicyError(f"image repository is missing: {reference}")
    else:
        registry = "docker.io"
        repository = parts

    if registry == "docker.io" and len(repository) == 1:
        repository.insert(0, "library")
    if any(not _NAME_COMPONENT.fullmatch(part) for part in repository):
        raise PolicyError(f"invalid image repository: {reference}")

    canonical = f"{registry}/{'/'.join(repository)}"
    if tag:
        canonical += f":{tag}"
    elif not digest:
        canonical += ":latest"
    if digest:
        canonical += f"@{digest}"
    return canonical


def _reject_yaml_features(text: str) -> None:
    try:
        for event in yaml.parse(text):
            if isinstance(event, yaml.events.AliasEvent):
                raise PolicyError("YAML aliases are not supported")
            tag = getattr(event, "tag", None)
            if tag is not None:
                raise PolicyError("explicit YAML tags are not supported")
    except yaml.YAMLError as exc:
        raise PolicyError(f"invalid YAML: {exc}") from exc


def load_policy(path: Path) -> list[str]:
    before = path.stat()
    if not stat.S_ISREG(before.st_mode):
        raise PolicyError(f"policy is not a regular file: {path}")
    text = path.read_text(encoding="utf-8")
    after = path.stat()
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
    ):
        raise PolicyError(f"policy changed while being read: {path}")

    _reject_yaml_features(text)
    try:
        data = yaml.safe_load(text)
    except yaml.YAMLError as exc:
        raise PolicyError(f"invalid YAML: {exc}") from exc
    if not isinstance(data, dict) or set(data) != {"schemaVersion", "allowedImages"}:
        raise PolicyError("policy must contain only schemaVersion and allowedImages")
    if type(data["schemaVersion"]) is not int or data["schemaVersion"] != 1:
        raise PolicyError("schemaVersion must be integer 1")
    if not isinstance(data["allowedImages"], list):
        raise PolicyError("allowedImages must be a list")

    images: list[str] = []
    seen: set[str] = set()
    for value in data["allowedImages"]:
        if not isinstance(value, str):
            raise PolicyError("every allowedImages entry must be a string")
        canonical = canonical_image(value)
        if canonical in seen:
            raise PolicyError(f"duplicate canonical image reference: {canonical}")
        seen.add(canonical)
        images.append(canonical)
    return sorted(images)


def build_rego(images: list[str], policy_id: str) -> str:
    lines = [
        "package docker",
        "",
        "default allow := false",
        "",
        "allow if { not input.image }",
        "allow if { input.image; allowed_image }",
        "",
    ]
    if images:
        for image in images:
            lines.append(f"allowed_image if input.image.ref == {json.dumps(image)}")
    else:
        lines.append("allowed_image := false")
    lines.extend(
        [
            "",
            "decision := {\"allow\": allow, \"deny_msg\": deny_msg}",
            "",
            "deny_msg contains msg if {",
            "  input.image",
            "  not allowed_image",
            f"  msg := sprintf(\"image %s is not approved by omp-sbx policy {policy_id}\", [input.image.ref])",
            "}",
            "",
        ]
    )
    return "\n".join(lines)


def atomic_write(path: Path, content: str, mode: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def compile_policy(input_path: Path, output_dir: Path) -> None:
    images = load_policy(input_path)
    digest_input = "".join(f"{image}\n" for image in images).encode()
    policy_id = f"sha256:{hashlib.sha256(digest_input).hexdigest()}"
    payload = {"schemaVersion": 1, "id": policy_id, "images": images}
    atomic_write(output_dir / "policy.json", json.dumps(payload, separators=(",", ":")) + "\n", 0o444)
    atomic_write(output_dir / "build-policy.rego", build_rego(images, policy_id), 0o444)
    atomic_write(output_dir / "policy-id", policy_id + "\n", 0o444)


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    compiler = subparsers.add_parser("compile")
    compiler.add_argument("--input", type=Path, required=True)
    compiler.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        compile_policy(args.input, args.output_dir)
    except (OSError, PolicyError) as exc:
        print(f"omp-sbx: invalid Docker image policy: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
